import { listen } from "@tauri-apps/api/event";
import { registerQuickAction, unregisterAllActions } from "@/apps/actions/registry";
import { registerApp, unregisterAll } from "@/apps/registry";
import type { AppIconDescriptor, QuickAction, Registration, TempoApp } from "@/apps/types";
import { api } from "@/lib/api";
import type {
  PluginActionContribution,
  PluginAppContribution,
  PluginContributionBundle,
} from "@/types";

const CONTRIBUTIONS_CHANGED_EVENT = "plugin-contributions-changed";

/** Strip the `{pluginId}/` prefix a runtime id carries (design §4.1) to get the plugin-local id. */
function localIdFromRuntimeId(runtimeId: string, pluginId: string): string {
  const prefix = `${pluginId}/`;
  return runtimeId.startsWith(prefix) ? runtimeId.slice(prefix.length) : runtimeId;
}

function iconFor(iconUrl: string | null | undefined): AppIconDescriptor {
  return { type: "file", path: iconUrl ?? "", url: iconUrl ?? undefined };
}

function toTempoApp(pluginId: string, contribution: PluginAppContribution): TempoApp {
  return {
    id: contribution.id,
    name: contribution.name,
    keywords: contribution.keywords,
    icon: iconFor(contribution.iconUrl),
    source: "plugin",
    pluginId,
    defaultSize: contribution.defaultSize
      ? {
          width: contribution.defaultSize.width ?? undefined,
          height: contribution.defaultSize.height ?? undefined,
        }
      : undefined,
    persistSession: contribution.persistSession,
    sessionVersion: contribution.sessionVersion ?? undefined,
    ui: {
      type: "plugin-webview",
      entryPath: contribution.entryPath,
      localAppId: contribution.localId,
    },
  };
}

function toQuickAction(pluginId: string, contribution: PluginActionContribution): QuickAction {
  const localCommandId = localIdFromRuntimeId(contribution.commandId, pluginId);
  const titleTemplate = contribution.titleTemplate ?? undefined;
  return {
    id: contribution.id,
    name: contribution.name,
    keywords: contribution.keywords,
    icon: iconFor(contribution.iconUrl),
    source: "plugin",
    pluginId,
    requiresQuery: contribution.requiresQuery,
    titleTemplate,
    title: titleTemplate ? (query) => titleTemplate.replace("{query}", query) : undefined,
    run: async (ctx) => {
      await api.pluginCallCommand(pluginId, localCommandId, { query: ctx.query });
      await ctx.hideAndReset();
    },
  };
}

function applyBundle(bundle: PluginContributionBundle) {
  unregisterAll(bundle.pluginId);
  unregisterAllActions(bundle.pluginId);
  for (const app of bundle.apps) {
    registerApp(bundle.pluginId, toTempoApp(bundle.pluginId, app));
  }
  for (const action of bundle.actions) {
    registerQuickAction(toQuickAction(bundle.pluginId, action), bundle.pluginId);
  }
}

function applyBundles(bundles: PluginContributionBundle[], previousPluginIds: Set<string>) {
  const nextPluginIds = new Set(bundles.map((bundle) => bundle.pluginId));
  for (const pluginId of previousPluginIds) {
    if (!nextPluginIds.has(pluginId)) {
      unregisterAll(pluginId);
      unregisterAllActions(pluginId);
    }
  }
  for (const bundle of bundles) {
    applyBundle(bundle);
  }
  return nextPluginIds;
}

/**
 * Loads declarative plugin contributes into the app/action registries and keeps them in sync
 * with `plugin-contributions-changed` (emitted after enable/disable/import/uninstall).
 * Call once from the command palette root; safe to call multiple times (idempotent per plugin).
 */
export function startPluginContributionSync(): Registration {
  let knownPluginIds = new Set<string>();
  let disposed = false;

  const reload = async () => {
    try {
      const bundles = await api.listPluginContributions();
      if (disposed) return;
      knownPluginIds = applyBundles(bundles, knownPluginIds);
    } catch (error) {
      console.error("failed to load plugin contributions", error);
    }
  };

  void reload();

  const unlistenPromise = listen(CONTRIBUTIONS_CHANGED_EVENT, () => void reload());

  return {
    dispose() {
      disposed = true;
      for (const pluginId of knownPluginIds) {
        unregisterAll(pluginId);
        unregisterAllActions(pluginId);
      }
      void unlistenPromise.then((unlisten) => unlisten());
    },
  };
}
