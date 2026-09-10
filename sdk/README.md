# tempo-plugin-sdk

Application model and typed Host APIs for Tempo plugins.

UI plugins connect after Tempo has supplied the page context:

```ts
import { connect } from "tempo-plugin-sdk/ui";

const app = await connect();
await app.notify.show({ title: `Tempo API ${app.context.apiVersion}` });
```

Runtime plugins declare their setup and cleanup in one place:

```ts
import { defineRuntime } from "tempo-plugin-sdk/runtime";

defineRuntime(({ commands, events, onDispose }) => {
  commands.register("run", async () => ({ ok: true }));
  onDispose(events.on("clipboard.changed", console.log));
});
```

Hybrid plugins can share an optional IPC contract:

```ts
type PluginIpc = {
  invokes: {
    greet: (name: string) => { message: string };
  };
  messages: {
    refreshed: [at: Date];
  };
};
```

Pass it to `connect<PluginIpc>()` and `defineRuntime<PluginIpc>()` to type channel names, arguments, and invoke results on both sides. Omitting the contract keeps dynamic string channels available.

The SDK uses APIs injected by Tempo. It does not request or bypass plugin permissions.

Vite projects can use the SDK-owned development transport and manifest helper:

```ts
import { defineConfig } from "vite";
import { tempoPlugin } from "tempo-plugin-sdk/vite";

export default defineConfig({
  plugins: [tempoPlugin()],
});
```

`tempoPlugin()` injects the SDK-owned transport during development and copies `manifest.json` during builds. Production plugin pages receive the versioned transport from Tempo, so generated projects do not need `.tempo` bridge files.
