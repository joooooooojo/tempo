import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { cp, mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import net from "node:net";
import path from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

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
  assert.equal(manifest.engines.pluginApi, "^2.0.0");
  const policy = manifest.permissions ?? {};
  const data = await mkdtemp(path.join(scratch, "data-"));
  const sockets = new Set();
  const responses = new Map();
  const hostCalls = [];
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
            if (frame.method === "notify.show") assert.equal(policy.host?.notify, true, "notification not declared");
            else if (frame.method === "storage.plugin.get") result = {value:{"default-who":"Deno"}};
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
  const reads = [packageDir, ...(policy.read?.includes("$DATA") ? [data] : [])];
  const args = ["run", "--no-config", "--no-lock", "--no-prompt", "--cached-only", "--no-remote", "--no-npm", "--node-modules-dir=none",
    `--allow-read=${reads.join(",")}`, `--allow-net=127.0.0.1:${port}`,
    ...(policy.write?.includes("$DATA") ? [`--allow-write=${data}`] : []), bootstrap];
  const env = {DENO_DIR:path.join(scratch,"cache")};
  for (const key of ["SystemRoot", "WINDIR", "TEMP", "TMP"]) if (process.env[key]) env[key] = process.env[key];
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
    t.after(() => rm(scratch,{recursive:true,force:true}));
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
    assert.equal(manifest.engines.pluginApi,"^2.0.0");
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

test("Hello demo runs Command, MCP and IPC with declared write/notify permissions",{skip:!deno},async t => {
  const scratch = await mkdtemp(path.join(root,".plugin-template-test-"));
  t.after(() => rm(scratch,{recursive:true,force:true}));
  const calls = [{type:"invoke",id:"command",commandId:"hello",params:{who:"Command"}},
    {type:"mcp-invoke",id:"mcp",toolName:"say-hello",arguments:{who:"MCP"}},
    {type:"ipc-invoke",id:"ipc",channel:"greet",args:[{who:"IPC"}]}];
  const result = await runPlugin(t,path.join(root,"examples/plugins/com.example.hello"),scratch,calls);
  const log = await readFile(path.join(result.data,"hello.log"),"utf8");
  for (const who of ["Command", "MCP", "IPC"]) assert.ok(log.includes(`Hello, ${who}!`));
  assert.equal(result.hostCalls.filter(method => method === "notify.show").length,3);
});
