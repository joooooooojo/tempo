# @tempo/sdk

Typed bindings for APIs injected by the Tempo plugin host.

```ts
import { tempo } from "@tempo/sdk/ui";

await tempo.ready();
await tempo.notify.show({ title: "Hello from Tempo" });
```

Runtime plugins use the separate Deno-compatible entry:

```ts
import { onMounted, tempo } from "@tempo/sdk/runtime";

onMounted(() => {
  tempo.commands.register("run", async () => ({ ok: true }));
});
```

The SDK exposes the host globals as typed module exports. It does not request or bypass plugin permissions.
