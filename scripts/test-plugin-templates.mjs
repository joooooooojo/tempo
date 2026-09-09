import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import {
  existsSync, mkdirSync, readFileSync, readdirSync, renameSync, rmSync, statSync, writeFileSync,
} from "node:fs";
import { cp, mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import net from "node:net";
import path from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { scaDecode } from "../plugin-runtime/structured-clone.mjs";

const root = fileURLToPath(new URL("../", import.meta.url));
const deno = process.env.TEMPO_PLUGIN_DENO_PATH;
const release = JSON.parse(await readFile(path.join(root, "templates/plugins/release.json"), "utf8"));
const bootstrap = path.join(root, "plugin-runtime/bootstrap.mjs");

async function runNode(script, args, cwd) {
  const child = spawn(process.execPath, [path.join(root, script), ...args], { cwd, windowsHide: true });
  let output = "";
  child.stdout.on("data", chunk => { output += chunk; });
  child.stderr.on("data", chunk => { output += chunk; });
  const timer = setTimeout(() => child.kill(), 60000);
  try { const [code] = await once(child, "close"); assert.equal(code, 0, output); }
  finally { clearTimeout(timer); }
}

async function renderTree(directory, kind) {
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const target = path.join(directory, entry.name);
    if (entry.isDirectory()) { await renderTree(target, kind); continue; }
    const text = await readFile(target, "utf8");
    await writeFile(target, text.replaceAll("__PLUGIN_ID__", `com.example.test-${kind}`)
      .replaceAll("__PLUGIN_NAME__", "Template Test").replaceAll("__PACKAGE_NAME__", `template-${kind}`)
      .replaceAll("__MANIFEST_SCHEMA_URL__", `https://example.com/releases/${release.version}/plugin-manifest.schema.json`));
  }
}

function encode(value) {
  const body = Buffer.from(JSON.stringify(value));
  const header = Buffer.alloc(4);
  header.writeUInt32BE(body.length);
  return Buffer.concat([header, body]);
}

async function runPlugin(t, packageDir, scratch, calls) {
  const manifest = JSON.parse(await readFile(path.join(packageDir, "manifest.json"), "utf8"));
  assert.equal(manifest.manifestVersion, 2);
  assert.equal(manifest.engines.pluginApi, "^2.1.0");
  const policy = manifest.permissions ?? {};
  const data = await mkdtemp(path.join(scratch, "data-"));
  const sockets = new Set();
  const responses = new Map();
  const hostCalls = [];
  const hostPath = relative => path.join(data,...String(relative ?? "").split("/"));
  let failure;
  const server = net.createServer(socket => {
    sockets.add(socket);
    socket.on("error", () => {});
    let buffer = Buffer.alloc(0);
    socket.on("data", chunk => {
      try {
        buffer = Buffer.concat([buffer, chunk]);
        while (buffer.length >= 4 && buffer.length >= buffer.readUInt32BE(0) + 4) {
          const size = buffer.readUInt32BE(0);
          const frame = JSON.parse(buffer.subarray(4, size + 4));
          buffer = buffer.subarray(size + 4);
          if (frame.type === "handshake") {
            assert.equal(frame.token, "template-test");
            socket.write(encode({type:"response",id:"handshake",ok:true,result:{}}));
          } else if (frame.type === "ready") {
            assert.equal(frame.ok, true, JSON.stringify(frame));
            for (const call of calls) socket.write(encode(call));
          } else if (frame.type === "request") {
            hostCalls.push(frame.method);
            let result = {};
            if (frame.method === "notify.show") result = {};
            else if (frame.method === "storage.plugin.get") result = {value:{"default-who":"Deno"}};
            else if (frame.method === "files.mkdir") {
              mkdirSync(hostPath(frame.params.path),{recursive:frame.params.recursive === true});
              result = null;
            }
            else if (frame.method === "files.writeText" || frame.method === "files.writeBytes") {
              writeFileSync(hostPath(frame.params.path),Buffer.from(frame.params.base64,"base64"));
              result = null;
            } else if (frame.method === "files.readText" || frame.method === "files.readBytes") {
              result = {base64:readFileSync(hostPath(frame.params.path)).toString("base64")};
            } else if (frame.method === "files.stat") {
              const target = hostPath(frame.params.path);
              const stat = existsSync(target) ? statSync(target) : null;
              result = {stat:stat === null ? null : {
                path:frame.params.path,type:stat.isFile() ? "file" : stat.isDirectory() ? "directory" : "other",
                size:stat.isFile() ? stat.size : null,modifiedAt:stat.mtime.toISOString(),
              }};
            } else if (frame.method === "files.list") {
              const relative = String(frame.params.path ?? "");
              const entries = readdirSync(hostPath(relative),{withFileTypes:true}).map(entry => {
                const entryRelative = relative ? `${relative}/${entry.name}` : entry.name;
                const stat = statSync(hostPath(entryRelative));
                return {name:entry.name,path:entryRelative,
                  type:entry.isFile() ? "file" : entry.isDirectory() ? "directory" : "other",
                  size:entry.isFile() ? stat.size : null,modifiedAt:stat.mtime.toISOString()};
              });
              result = {entries};
            } else if (frame.method === "files.rename") {
              renameSync(hostPath(frame.params.from),hostPath(frame.params.to));
              result = null;
            } else if (frame.method === "files.remove") {
              rmSync(hostPath(frame.params.path),{recursive:frame.params.recursive === true});
              result = null;
            }
            else throw new Error(`unexpected Host method ${frame.method}`);
            socket.write(encode({type:"response",id:frame.id,ok:true,result}));
          } else if (frame.type === "response") {
            assert.equal(frame.ok, true, JSON.stringify(frame));
            responses.set(frame.id, frame.result);
            if (responses.size === calls.length) socket.write(encode({type:"shutdown"}));
          }
        }
      } catch (error) { failure = error; socket.destroy(); }
    });
  });
  t.after(() => { for (const socket of sockets) socket.destroy(); server.close(); });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const port = server.address().port;
  const permissionNames = ["read","write","net","env","sys","run","ffi","import"];
  const selected = new Set(Array.isArray(policy) ? policy : []);
  const allowAll = permissionNames.every(permission => selected.has(permission));
  const args = allowAll
    ? ["run","--no-config","--no-lock","--no-prompt","-A",bootstrap]
    : ["run", "--no-config", "--no-lock", "--no-prompt",
      ...(!selected.has("import") ? ["--cached-only", "--no-remote", "--no-npm"] : []),
      "--node-modules-dir=none",
      selected.has("read") ? "--allow-read" : `--allow-read=${packageDir}`,
      selected.has("net") ? "--allow-net" : `--allow-net=127.0.0.1:${port}`,
      ...["write","env","sys","run","ffi","import"].filter(permission => selected.has(permission)).map(permission => `--allow-${permission}`),
      bootstrap];
  const env = selected.has("env") || allowAll ? {...process.env} : {};
  env.DENO_DIR = path.join(data,"cache");
  if (!selected.has("env") && !allowAll) {
    for (const key of ["SystemRoot", "WINDIR", "TEMP", "TMP"]) if (process.env[key]) env[key] = process.env[key];
  }
  const child = spawn(deno, args, {cwd:packageDir, env, windowsHide:true});
  const timer = setTimeout(() => child.kill(), 12000);
  t.after(() => { clearTimeout(timer); if (child.exitCode === null) child.kill(); });
  let stderr = "";
  child.stderr.on("data", chunk => { stderr += chunk; });
  child.stdout.resume();
  child.stdin.on("error", () => {});
  child.stdin.end(JSON.stringify({socketAddress:{host:"127.0.0.1",port},token:"template-test",pluginId:manifest.id,
    mainPath:path.join(packageDir,manifest.main),dataPath:data,runtimeVersion:"2.9.6"}) + "\n");
  const [code, signal] = await once(child,"close");
  clearTimeout(timer);
  if (failure) throw failure;
  assert.equal(signal,null,stderr);
  assert.equal(code,0,stderr);
  assert.equal(responses.size,calls.length,stderr);
  return {data, responses, hostCalls};
}

for (const kind of ["ui", "hybrid", "headless"]) {
  test(`${kind} template typechecks and builds`, async t => {
    // Keep the scratch project beneath the repository so the tested toolchain resolves normally.
    const scratch = await mkdtemp(path.join(root,".plugin-template-test-"));
    t.after(() => rm(scratch,{recursive:true,force:true,maxRetries:5,retryDelay:100}));
    const project = path.join(scratch,"project");
    await cp(path.join(root,"templates/plugins",kind),project,{recursive:true});
    await renderTree(project,kind);
    if (kind === "hybrid") {
      const solution = JSON.parse(await readFile(path.join(project, "tsconfig.json"), "utf8"));
      assert.deepEqual(solution.files, []);
      assert.deepEqual(solution.references.map(ref => ref.path), ["./tsconfig.ui.json", "./tsconfig.runtime.json"]);
      await writeFile(path.join(project, "src/ui/type-isolation.ts"),
        "// @ts-expect-error Node globals must not leak into UI.\nprocess.cwd();\nexport {};\n");
      await writeFile(path.join(project, "src/runtime/type-isolation.ts"),
        "// @ts-expect-error DOM globals must not leak into Runtime.\ndocument.title;\nexport {};\n");
      await runNode("node_modules/typescript/bin/tsc", ["-b"], project);
      await runNode("node_modules/typescript/bin/tsc", ["-b"], project);
    } else {
      await runNode("node_modules/typescript/bin/tsc", ["-p", "tsconfig.json"], project);
    }
    await runNode("node_modules/vite/bin/vite.js",["build"],project);
    if (kind === "hybrid") await runNode("node_modules/vite/bin/vite.js",["build","--config","vite.runtime.config.ts"],project);
    const manifest = JSON.parse(await readFile(path.join(project,"dist/manifest.json"),"utf8"));
    assert.equal(manifest.manifestVersion,2);
    assert.equal(manifest.engines.pluginApi,"^2.1.0");
    if (kind !== "ui") await t.test("built output executes on restricted Deno",{skip:!deno},async t => {
      const commandId = kind === "hybrid" ? "greet" : "run";
      const toolName = kind === "hybrid" ? "greet-tool" : "run-tool";
      const calls = [{type:"invoke",id:"command",commandId,params:{name:"Deno"}},
        {type:"mcp-invoke",id:"mcp",toolName,arguments:{name:"Deno"}}];
      if (kind === "hybrid") calls.push({type:"ipc-invoke",id:"ipc",channel:"greet",args:[{name:"Deno"}]});
      const result = await runPlugin(t,path.join(project,"dist"),scratch,calls);
      if (kind === "headless") assert.ok(result.hostCalls.includes("notify.show"));
      else assert.equal(result.responses.get("command").message,"Hello, Deno!");
    });
  });
}

test("Hello demo declares no Deno permissions",async () => {
  const manifest = JSON.parse(await readFile(
    path.join(root,"examples/plugins/com.example.hello/manifest.json"),"utf8",
  ));
  assert.equal(manifest.version,"2.1.0");
  assert.equal(manifest.engines.pluginApi,"^2.1.0");
  assert.deepEqual(manifest.permissions,[]);
});

test("Hello demo uses Host files and blocks undeclared Deno permissions",{skip:!deno},async t => {
  const scratch = await mkdtemp(path.join(root,".plugin-template-test-"));
  t.after(() => rm(scratch,{recursive:true,force:true,maxRetries:5,retryDelay:100}));
  const calls = [{type:"invoke",id:"command",commandId:"hello",params:{who:"Command"}},
    {type:"mcp-invoke",id:"mcp",toolName:"say-hello",arguments:{who:"MCP"}},
    {type:"ipc-invoke",id:"ipc",channel:"greet",args:[{who:"IPC"}]},
    {type:"ipc-invoke",id:"permissions",channel:"permission-probe",args:[]}];
  const result = await runPlugin(t,path.join(root,"examples/plugins/com.example.hello"),scratch,calls);
  const log = await readFile(path.join(result.data,"hello.log"),"utf8");
  for (const who of ["Command", "MCP", "IPC"]) assert.ok(log.includes(`Hello, ${who}!`));
  assert.equal(result.hostCalls.filter(method => method === "notify.show").length,3);
  const permissions = scaDecode(result.responses.get("permissions"));
  assert.deepEqual(permissions.declared,[]);
  assert.equal(permissions.host.ok,true);
  assert.deepEqual(permissions.host.bytes,[0,1,2,255]);
  assert.equal(permissions.direct.read.allowed,false);
  assert.equal(permissions.direct.write.allowed,false);
  for (const [name,state] of Object.entries(permissions.permissions)) {
    assert.notEqual(state,"granted",`${name} should remain blocked`);
  }

  for (const [mode,policy] of [
    ["granular",["read","write","net","env"]],
    ["all",["read","write","net","env","sys","run","ffi","import"]],
  ]) {
    const packageDir = path.join(scratch,mode);
    await cp(path.join(root,"examples/plugins/com.example.hello"),packageDir,{recursive:true});
    const manifestPath = path.join(packageDir,"manifest.json");
    const manifest = JSON.parse(await readFile(manifestPath,"utf8"));
    manifest.permissions = policy;
    await writeFile(manifestPath,`${JSON.stringify(manifest,null,2)}\n`);
    const grantedResult = await runPlugin(t,packageDir,scratch,[
      {type:"ipc-invoke",id:"permissions",channel:"permission-probe",args:[]},
    ]);
    const granted = scaDecode(grantedResult.responses.get("permissions"));
    assert.deepEqual(granted.declared,policy);
    assert.equal(granted.host.ok,true);
    assert.equal(granted.direct.read.allowed,true);
    assert.equal(granted.direct.write.allowed,true);
    for (const name of ["read","write","net","env"]) {
      assert.equal(granted.permissions[name],"granted",`${mode} should grant ${name}`);
    }
    for (const name of ["sys","run","ffi"]) {
      if (mode === "all") assert.equal(granted.permissions[name],"granted");
      else assert.notEqual(granted.permissions[name],"granted",`${mode} should block ${name}`);
    }
    assert.equal(granted.permissions.import,mode === "all" ? "granted" : "denied");
  }
});
