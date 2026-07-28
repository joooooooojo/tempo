import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isBlurHideSuppressed } from "@/lib/blurHideGuard";
import {
  LoaderCircle,
  Pin,
  PinOff,
  ArrowLeft,
  Pencil,
} from "lucide-react";
import { Toaster, toast } from "sonner";
import { AppIcon } from "@/components/AppIcon";
import { ClipboardFileGlyph } from "@/components/clipboard/ClipboardFileGlyph";
import { ReminderDialog } from "@/components/ReminderDialog";
import { Button } from "@/components/ui/button";
import { FollowTooltip } from "@/components/ui/follow-tooltip";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Skeleton } from "@/components/ui/skeleton";
import {
  listVisibleQuickActions,
  quickActionUsageId,
} from "@/apps/actions/registry";
import { AppIconView } from "@/apps/icon";
import { BuiltinAppNavigationProvider } from "@/apps/navigation";
import { PluginAppHost } from "@/apps/PluginAppHost";
import { startPluginContributionSync } from "@/apps/plugins/syncContributions";
import { getApp as getBuiltinApp, listApps as listBuiltinApps, subscribeApps } from "@/apps/registry";
import {
  clearMainPanelSession,
  resolveRestorableMainPanelSession,
  writeMainPanelSession,
} from "@/apps/mainPanelSession";
import {
  resolveOpenAppParams,
  type AppRectValue,
  type BuiltinApp,
  type OpenBuiltinAppOptions,
  type QuickAction,
} from "@/apps/types";
import { api } from "@/lib/api";
import { notifyUser } from "@/lib/notifications";
import { playNotificationSound } from "@/lib/sound";
import {
  applyTheme,
  emitThemeChange,
  subscribeThemeChanges,
  watchSystemTheme,
} from "@/lib/theme";
import { appToastOptions } from "@/lib/toastOptions";
import {
  resolveQuickActionInput,
  resolveQuickActionQuery,
  seedToMainPanelChip,
  shouldInlineClipboardText,
  type MainPanelClipboardChip,
} from "@/lib/mainPanelClipboardSeed";
import { formatClipboardFilesPreview } from "@/lib/clipboardFiles";
import { cn } from "@/lib/utils";
import type {
  MainPanelClipboardSeed,
  LauncherApp,
  LauncherUsageItem,
  ReminderEvent,
  Settings,
} from "@/types";

const GRID_COLUMNS = 9;
const RECENT_COLLAPSED_COUNT = GRID_COLUMNS * 2;
const PINNED_COLLAPSED_COUNT = GRID_COLUMNS;
const SEARCH_COLLAPSED_COUNT = GRID_COLUMNS * 2;
const MAX_SEARCH_RESULTS = GRID_COLUMNS * 4;
const SEARCH_QUERY_DEBOUNCE_MS = 80;
/** Typing changes result height often; native window resize each time feels like input lag. */
const MAIN_PANEL_RESIZE_DEBOUNCE_MS = 100;
/** Pending clipboard chip (text/image/file) is cleared if panel stays closed this long. */
const CLIPBOARD_CHIP_STALE_MS = 30_000;
const SEARCH_WIDTH = 800;
/** Fallback only when content has not mounted yet; prefer measured scrollHeight. */
const SEARCH_FALLBACK_HEIGHT = 370;
const DEFAULT_APP_HEIGHT = 580;
const APP_CHROME_HEIGHT = 58;
const BUILTIN_USAGE_PREFIX = "builtin:";
const PLUGIN_USAGE_PREFIX = "plugin:";
/** Skip host padding: edge-to-edge builtins, and all plugins (authors pad themselves). */
const FLUSH_APP_IDS = new Set(["hosts", "translate", "port-manager", "settings"]);
/** Fill host via h-full/flex ? do not wrap in ScrollArea (breaks height chain). */
const FILL_HEIGHT_APP_IDS = new Set([
  "hosts",
  "translate",
  "port-manager",
  "todo",
  "settings",
]);

type MainPanelMode = "search" | "app";

type MainPanelSelection =
  | { key: string; kind: "app"; app: LauncherApp }
  | { key: string; kind: "builtin"; app: BuiltinApp }
  | { key: string; kind: "action"; action: QuickAction };

type RecentEntry =
  | {
      key: string;
      kind: "app";
      app: LauncherApp;
      last_used_at: string | null;
      use_count: number;
    }
  | {
      key: string;
      kind: "builtin";
      app: BuiltinApp;
      last_used_at: string | null;
      use_count: number;
    };

type SearchAppEntry =
  | { key: string; kind: "builtin"; app: BuiltinApp }
  | { key: string; kind: "app"; app: LauncherApp };

type OpenAppPayload = {
  appId: string;
  createSnippet?: boolean;
  params?: Record<string, unknown>;
};

function builtinUsageId(appId: string) {
  return `${BUILTIN_USAGE_PREFIX}${appId}`;
}

export function MainPanelPage() {
  const [mode, setMode] = useState<MainPanelMode>("search");
  const [activeAppId, setActiveAppId] = useState<string | null>(null);
  const [activeAppParams, setActiveAppParams] = useState<Record<string, unknown>>({});
  const [openCreateSnippet, setOpenCreateSnippet] = useState(false);
  const [initialTranslateText, setInitialTranslateText] = useState<string | undefined>();
  const [apps, setApps] = useState<LauncherApp[]>([]);
  const [usageItems, setUsageItems] = useState<LauncherUsageItem[]>([]);
  const [query, setQuery] = useState("");
  const [clipboardChip, setClipboardChip] = useState<MainPanelClipboardChip | null>(null);
  const [loading, setLoading] = useState(true);
  const [recentExpanded, setRecentExpanded] = useState(false);
  const [pinnedExpanded, setPinnedExpanded] = useState(false);
  const [searchExpanded, setSearchExpanded] = useState(false);
  const [matchedSearchApps, setMatchedSearchApps] = useState<SearchAppEntry[]>([]);
  const [searchIndexRevision, setSearchIndexRevision] = useState(0);
  const [launcherIndexRevision, setLauncherIndexRevision] = useState(0);
  const [openRevision, setOpenRevision] = useState(0);
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [pendingKey, setPendingKey] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [reminder, setReminder] = useState<ReminderEvent | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const inputMeasureRef = useRef<HTMLSpanElement>(null);
  const inputWrapperRef = useRef<HTMLDivElement>(null);
  /** Ignore Enter/arrows while IME is composing (macOS 拼音选词确认也会发 Enter). */
  const imeComposingRef = useRef(false);
  const queryRef = useRef(query);
  const clipboardChipRef = useRef<MainPanelClipboardChip | null>(null);
  /** Timestamp when panel was hidden while a clipboard chip was active. */
  const clipboardChipHiddenAtRef = useRef<number | null>(null);
  const panelVisibleRef = useRef(true);
  const contentRef = useRef<HTMLDivElement>(null);
  const pendingRef = useRef<string | null>(null);
  const modeRef = useRef<MainPanelMode>("search");
  const activeAppIdRef = useRef<string | null>(null);
  /** After leaving a plugin, size once to measured search content (skip placeholder 370). */
  const needsSearchSizeRef = useRef(false);
  const isTauri = isTauriRuntime();

  const focusSearchInputAtEnd = useCallback(() => {
    const input = inputRef.current;
    if (!input) return;
    input.focus();
    const end = input.value.length;
    input.setSelectionRange(end, end);
  }, []);

  /** Match ZTools: keep the input interactive and move the window through its surrounding field. */
  const beginMainPanelDrag = useCallback(
    async (event: React.MouseEvent) => {
      if (event.button !== 0 || !isTauri) return;
      const target = event.target;
      if (!(target instanceof Element)) return;
      if (target.closest("input, button, a, [data-no-drag]")) return;

      const startScreenX = event.screenX;
      const startScreenY = event.screenY;
      let dragReady = true;
      let dragging = false;
      let moved = false;
      let offsetX = 0;
      let offsetY = 0;
      let lastX = 0;
      let lastY = 0;

      function cleanup() {
        document.removeEventListener("mousemove", onMove);
        document.removeEventListener("mouseup", onEnd);
      }

      function cancelDrag() {
        dragReady = false;
        dragging = false;
        cleanup();
      }

      function finishDrag(endTarget?: EventTarget | null) {
        if (!dragReady && !dragging) return;
        const shouldSave = moved;
        cancelDrag();
        if (shouldSave) {
          void api
            .setMainPanelPosition(lastX, lastY)
            .then(() => api.saveMainPanelPosition())
            .catch(() => undefined);
        }
        if (
          modeRef.current === "search" &&
          (!(endTarget instanceof Element) || !endTarget.closest("input, button, a"))
        ) {
          inputRef.current?.focus({ preventScroll: true });
        }
      }

      function onMove(moveEvent: MouseEvent) {
        if (moveEvent.buttons === 0) {
          finishDrag(moveEvent.target);
          return;
        }
        if (!dragging) return;
        lastX = moveEvent.screenX - offsetX;
        lastY = moveEvent.screenY - offsetY;
        moved = true;
        void api.setMainPanelPosition(lastX, lastY);
      }

      function onEnd(endEvent: MouseEvent) {
        finishDrag(endEvent.target);
      }

      // Register synchronously so mouseup cannot be lost while position is requested.
      document.addEventListener("mousemove", onMove);
      document.addEventListener("mouseup", onEnd);

      try {
        const position = await api.getMainPanelPosition();
        if (!dragReady) return;
        offsetX = startScreenX - position.x;
        offsetY = startScreenY - position.y;
        dragging = true;
      } catch {
        cancelDrag();
      }
    },
    [isTauri]
  );

  useLayoutEffect(() => {
    const input = inputRef.current;
    const measure = inputMeasureRef.current;
    const wrapper = inputWrapperRef.current;
    if (!input || !measure || !wrapper) return;

    const contentWidth = query ? measure.offsetWidth + 10 : 2;
    input.style.width = `${Math.max(2, Math.min(contentWidth, wrapper.clientWidth))}px`;
  }, [clipboardChip, mode, query]);

  const [appsRevision, setAppsRevision] = useState(0);
  const [disabledBuiltinIds, setDisabledBuiltinIds] = useState<Set<string>>(new Set());
  const builtinApps = useMemo(() => listBuiltinApps(), [appsRevision]);
  const enabledBuiltinApps = useMemo(
    () => builtinApps.filter((app) => !disabledBuiltinIds.has(app.id)),
    [builtinApps, disabledBuiltinIds]
  );
  const builtinAppsRef = useRef(enabledBuiltinApps);
  builtinAppsRef.current = enabledBuiltinApps;

  useEffect(() => {
    const unsubscribe = subscribeApps(() => setAppsRevision((current) => current + 1));
    return unsubscribe;
  }, []);

  const refreshDisabledBuiltins = useCallback(() => {
    if (!isTauri) return;
    void api
      .getSettings()
      .then((settings) => {
        setDisabledBuiltinIds(new Set(settings.disabled_builtin_apps ?? []));
      })
      .catch(() => {
        /* ignore */
      });
  }, [isTauri]);

  useEffect(() => {
    refreshDisabledBuiltins();
  }, [refreshDisabledBuiltins]);

  useEffect(() => {
    if (!isTauri) return;
    let cancelled = false;
    void api
      .syncMainPanelSearchContributions(
        enabledBuiltinApps.map((app) => ({
          id: app.id,
          name: app.name,
          keywords: app.keywords,
          source: app.source,
        }))
      )
      .then(() => {
        if (!cancelled) setSearchIndexRevision((current) => current + 1);
      })
      .catch((syncError) => {
        if (!cancelled) {
          setError(errorMessage(syncError, "无法更新搜索索引"));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [enabledBuiltinApps, isTauri]);

  useEffect(() => {
    if (!isTauri) return;
    const registration = startPluginContributionSync();
    return () => registration.dispose();
  }, [isTauri]);

  useEffect(() => {
    modeRef.current = mode;
  }, [mode]);

  useEffect(() => {
    activeAppIdRef.current = activeAppId;
  }, [activeAppId]);

  useEffect(() => {
    queryRef.current = query;
  }, [query]);

  useEffect(() => {
    clipboardChipRef.current = clipboardChip;
  }, [clipboardChip]);

  const resetSearchState = useCallback(() => {
    setQuery("");
    queryRef.current = "";
    setClipboardChip(null);
    clipboardChipRef.current = null;
    clipboardChipHiddenAtRef.current = null;
    setRecentExpanded(false);
    setPinnedExpanded(false);
    setSearchExpanded(false);
    setSelectedKey(null);
    pendingRef.current = null;
    setPendingKey(null);
    setError(null);
  }, []);

  const resetMainPanelState = useCallback(() => {
    toast.dismiss();
    resetSearchState();
    setMode("search");
    setActiveAppId(null);
    setActiveAppParams({});
    setOpenCreateSnippet(false);
    setInitialTranslateText(undefined);
  }, [resetSearchState]);

  const dismissClipboardSeed = useCallback(() => {
    if (!isTauri) return;
    void api.clearMainPanelClipboardSeed().catch(console.error);
  }, [isTauri]);

  const clearClipboardChipLocal = useCallback(() => {
    setClipboardChip(null);
    clipboardChipRef.current = null;
    clipboardChipHiddenAtRef.current = null;
  }, []);

  const applySeedToUi = useCallback(
    (seed: MainPanelClipboardSeed) => {
      if (seed.kind === "text" && seed.fullText) {
        if (shouldInlineClipboardText(seed.fullText)) {
          clearClipboardChipLocal();
          setQuery(seed.fullText.trim());
          queryRef.current = seed.fullText.trim();
        } else {
          const chip = seedToMainPanelChip(seed);
          setClipboardChip(chip);
          clipboardChipRef.current = chip;
          setQuery("");
          queryRef.current = "";
        }
        return;
      }
      if (seed.kind === "image") {
        const chip = seedToMainPanelChip(seed);
        setClipboardChip(chip);
        clipboardChipRef.current = chip;
        setQuery("");
        queryRef.current = "";
        return;
      }
      if (seed.kind === "file") {
        const chip = seedToMainPanelChip(seed);
        setClipboardChip(chip);
        clipboardChipRef.current = chip;
        setQuery("");
        queryRef.current = "";
      }
    },
    [clearClipboardChipLocal]
  );

  const applyClipboardSeedFromBackend = useCallback(
    async (options?: { replace?: boolean }) => {
      if (!isTauri) return;
      const replace = options?.replace === true;
      try {
        const seed = await api.getMainPanelClipboardSeed();
        if (!seed) {
          // Keep any blur-preserved chip when not forcing a blank state.
          return;
        }
        if (
          !replace &&
          (clipboardChipRef.current || queryRef.current.trim())
        ) {
          return;
        }
        applySeedToUi(seed);
        window.requestAnimationFrame(focusSearchInputAtEnd);
      } catch (error) {
        console.error(error);
      }
    },
    [applySeedToUi, focusSearchInputAtEnd, isTauri]
  );

  /** Manual Ctrl/Cmd+V: pull current OS clipboard even after auto-seed expired. */
  const handleManualPaste = useCallback(
    async (event: React.ClipboardEvent<HTMLInputElement>) => {
      if (!isTauri || modeRef.current !== "search") return;
      event.preventDefault();
      try {
        const seed = await api.seedMainPanelFromSystemClipboard();
        if (!seed) {
          toast.error("剪贴板中没有可粘贴的内容");
          return;
        }
        applySeedToUi(seed);
        window.requestAnimationFrame(focusSearchInputAtEnd);
      } catch (error) {
        console.error(error);
        toast.error(error instanceof Error ? error.message : "粘贴失败");
      }
    },
    [applySeedToUi, focusSearchInputAtEnd, isTauri]
  );

  const markClipboardChipHidden = useCallback(() => {
    if (clipboardChipRef.current) {
      clipboardChipHiddenAtRef.current = Date.now();
    }
    panelVisibleRef.current = false;
  }, []);

  const clipboardSeedForActions = useMemo((): MainPanelClipboardSeed | null => {
    if (!clipboardChip) return null;
    if (clipboardChip.kind === "text") {
      return { kind: "text", fullText: clipboardChip.fullText };
    }
    if (clipboardChip.kind === "file") {
      return {
        kind: "file",
        entryId: clipboardChip.entryId,
        paths: clipboardChip.paths,
      };
    }
    return {
      kind: "image",
      entryId: clipboardChip.entryId,
      imageUrl: clipboardChip.imageUrl,
      imageWidth: clipboardChip.imageWidth,
      imageHeight: clipboardChip.imageHeight,
    };
  }, [clipboardChip]);

  const hideAndResetMainPanel = useCallback(async () => {
    clearMainPanelSession();
    clipboardChipHiddenAtRef.current = null;
    panelVisibleRef.current = false;
    dismissClipboardSeed();
    await hideMainPanel();
    resetMainPanelState();
    // Search layout ResizeObserver updates size while the window is still hidden.
  }, [dismissClipboardSeed, resetMainPanelState]);

  /** Blur / outside click: keep search chip/query (and app session) for the next open. */
  const hidePreservingSession = useCallback(async () => {
    const appId = activeAppIdRef.current;
    if (modeRef.current === "app" && appId) {
      writeMainPanelSession(appId);
      panelVisibleRef.current = false;
      await hideMainPanel();
      return;
    }
    markClipboardChipHidden();
    await hideMainPanel();
  }, [markClipboardChipHidden]);

  const backToSearch = useCallback(() => {
    toast.dismiss();
    clearMainPanelSession();
    setMode("search");
    setActiveAppId(null);
    setActiveAppParams({});
    setOpenCreateSnippet(false);
    setInitialTranslateText(undefined);
    setError(null);
    setSelectedKey(null);
    if (isTauri) {
      // Defer size until search DOM is laid out so we jump once to measured
      // height instead of 370 -> ResizeObserver correction (height jitter).
      needsSearchSizeRef.current = true;
      void api.getLauncherUsage().then(setUsageItems).catch(console.error);
    }
    window.requestAnimationFrame(focusSearchInputAtEnd);
  }, [focusSearchInputAtEnd, isTauri]);

  const openBuiltinApp = useCallback(
    (appId: string, options?: OpenBuiltinAppOptions) => {
      const app = getBuiltinApp(appId);
      if (!app) return;
      const params = resolveOpenAppParams(options);

      if (isTauri && !options?.restore) {
        // Always record via Rust (local RFC3339) then refresh ? JS toISOString() is UTC
        // and lexicographic string sort put builtins after local +08:00 OS app timestamps.
        const usageId =
          app.source === "plugin" ? `plugin:${app.id}` : builtinUsageId(appId);
        void api
          .recordLauncherUsage(usageId)
          .then(() => api.getLauncherUsage())
          .then(setUsageItems)
          .catch((recordError) => {
            console.error(recordError);
          });
      }

      if (
        isTauri &&
        app.source === "plugin" &&
        app.pluginId &&
        app.ui.type === "plugin-webview" &&
        app.windowMode === "standalone"
      ) {
        clearMainPanelSession();
        void api
          .openPluginWindow({
            pluginId: app.pluginId,
            appId: app.ui.localAppId,
            params,
          })
          .then(hideAndResetMainPanel)
          .catch((openError) => setError(errorMessage(openError, "无法打开插件窗口")));
        return;
      }

      if (isTauri) {
        void api
          .setMainPanelRect(app.rect ?? {})
          .catch((rectError) => setError(errorMessage(rectError, "无法调整应用窗口")));
      }

      setMode("app");
      setActiveAppId(appId);
      setActiveAppParams(params);
      setOpenCreateSnippet(Boolean(params.createSnippet));
      const translateText =
        typeof params.initialTranslateText === "string"
          ? params.initialTranslateText.trim()
          : undefined;
      setInitialTranslateText(translateText || undefined);
      setQuery("");
      setError(null);
      writeMainPanelSession(appId);
      if (isTauri) needsSearchSizeRef.current = true;
    },
    [hideAndResetMainPanel, isTauri]
  );

  useEffect(() => {
    if (!openCreateSnippet) return;
    const timer = window.setTimeout(() => setOpenCreateSnippet(false), 100);
    return () => window.clearTimeout(timer);
  }, [openCreateSnippet, activeAppId]);

  const loadApps = useCallback(
    async (refresh = false) => {
      if (!isTauri) {
        setApps([]);
        setUsageItems([]);
        setLoading(false);
        return;
      }
      try {
        const [nextApps, nextUsage] = await Promise.all([
          refresh ? api.refreshLauncherApps() : api.getLauncherApps(),
          api.getLauncherUsage(),
        ]);
        setApps(nextApps);
        setLauncherIndexRevision((current) => current + 1);
        setUsageItems(nextUsage);
        setLoading(nextApps.length === 0);
        setError(null);
      } catch (loadError) {
        setError(errorMessage(loadError, "无法读取本机应用"));
        setLoading(false);
      }
    },
    [isTauri]
  );

  useEffect(() => {
    pendingRef.current = pendingKey;
  }, [pendingKey]);

  useEffect(() => {
    let currentTheme: Settings["theme"] = "system";
    const root = document.documentElement;
    root.classList.add("main-panel-window");
    document.body.classList.add("main-panel-window");
    if (isTauri) {
      void api
        .getSettings()
        .then((settings) => {
          currentTheme = settings.theme;
          applyTheme(currentTheme);
        })
        .catch(() => applyTheme("system"));
    } else {
      applyTheme("system");
    }
    const unsubscribeTheme = isTauri
      ? subscribeThemeChanges((theme) => {
          currentTheme = theme;
          applyTheme(theme);
        })
      : () => {};
    const unwatchSystemTheme = isTauri
      ? watchSystemTheme(
          () => currentTheme,
          () => void emitThemeChange("system")
        )
      : () => {};
    void loadApps();

    return () => {
      root.classList.remove("main-panel-window");
      document.body.classList.remove("main-panel-window");
      unwatchSystemTheme();
      unsubscribeTheme();
    };
  }, [isTauri, loadApps]);

  useEffect(() => {
    if (!isTauri) return;

    const unlistenReminder = listen<ReminderEvent>("reminder", (e) => {
      if (e.payload.type === "todo_due") {
        void api.getSettings().then((s) => {
          if (s.sound_enabled) playNotificationSound();
        });
        const leadText =
          e.payload.lead === "1d"
            ? "将在 1 天后截止"
            : e.payload.lead === "1h"
              ? "将在 1 小时后截止"
              : "已到截止时间";
        void notifyUser("待办提醒", `「${e.payload.title}」${leadText}`);
      }

      setReminder(e.payload);
    });

    const unlistenToast = listen<{ message: string }>("toast", (e) => {
      toast.info(e.payload.message);
    });

    const unlistenCreate = listen("snippets:create-request", () => {
      openBuiltinApp("snippets", { createSnippet: true });
    });
    const unlistenManage = listen("snippets:manage-request", () => {
      openBuiltinApp("snippets");
    });
    const unlistenOpenApp = listen<OpenAppPayload>("main-panel:open-app", (e) => {
      openBuiltinApp(e.payload.appId, {
        createSnippet: e.payload.createSnippet,
        params: e.payload.params,
      });
    });

    return () => {
      void unlistenReminder.then((fn) => fn());
      void unlistenToast.then((fn) => fn());
      void unlistenCreate.then((fn) => fn());
      void unlistenManage.then((fn) => fn());
      void unlistenOpenApp.then((fn) => fn());
    };
  }, [isTauri, openBuiltinApp]);

  useEffect(() => {
    if (!isTauri) {
      inputRef.current?.focus();
      return;
    }

    let disposed = false;
    let armed = false;
    let armTimer = 0;
    let unlistenBlur: (() => void) | undefined;
    const appWindow = getCurrentWindow();

    const focusSearchInput = focusSearchInputAtEnd;

    const restoreSessionIfNeeded = () => {
      if (modeRef.current === "app" && activeAppIdRef.current) {
        // Keep current size ? resizing here after show causes a visible flash.
        return true;
      }
      const session = resolveRestorableMainPanelSession();
      if (!session) return false;
      openBuiltinApp(session.appId, { restore: true });
      return true;
    };

    const prepareForOpen = () => {
      armed = false;
      window.clearTimeout(armTimer);
      panelVisibleRef.current = true;
      setOpenRevision((current) => current + 1);
      refreshDisabledBuiltins();
      const restored = restoreSessionIfNeeded();
      if (!restored && modeRef.current === "search") {
        setSelectedKey(null);

        const hiddenAt = clipboardChipHiddenAtRef.current;
        clipboardChipHiddenAtRef.current = null;
        const chipStale =
          Boolean(clipboardChipRef.current) &&
          hiddenAt != null &&
          Date.now() - hiddenAt >= CLIPBOARD_CHIP_STALE_MS;

        if (chipStale) {
          // Closed >30s with a pending chip: drop preserved UI, then allow a fresh seed.
          clearClipboardChipLocal();
          setQuery("");
          queryRef.current = "";
          void applyClipboardSeedFromBackend();
        } else if (clipboardChipRef.current) {
          // Panel was hidden (not visible): newer copies while closed may replace the chip.
          // Never replace while the panel stays open — copying panel content must not swap the seed.
          void applyClipboardSeedFromBackend({ replace: true });
        } else if (!queryRef.current.trim()) {
          void applyClipboardSeedFromBackend();
        }

        // The native window can lose focus while its startup page is still visible. DOM focus
        // alone cannot recover from that state, so reactivate the window before focusing input.
        void appWindow
          .setFocus()
          .catch(() => undefined)
          .finally(() => {
            window.requestAnimationFrame(focusSearchInput);
            // WebView focus can settle one tick after the native window becomes active.
            window.setTimeout(focusSearchInput, 50);
          });
      }
      armTimer = window.setTimeout(() => {
        armed = true;
      }, 220);
    };

    const unlistenOpen = listen("main-panel:open", prepareForOpen);
    const unlistenShortcutHide = listen("main-panel:shortcut-hide", () => {
      const appId = activeAppIdRef.current;
      if (modeRef.current === "app" && appId) {
        writeMainPanelSession(appId);
        panelVisibleRef.current = false;
        return;
      }
      clearMainPanelSession();
      // Keep search chip/query across shortcut hide, same as blur.
      markClipboardChipHidden();
    });
    const unlistenClipboard = listen("clipboard-update", () => {
      // While visible: never replace an existing chip (copying panel content must not swap it).
      // If the chip was cleared, allow a new/same copy to fill again.
      if (!panelVisibleRef.current) return;
      if (modeRef.current !== "search") return;
      if (clipboardChipRef.current) return;
      if (queryRef.current.trim()) return;
      void applyClipboardSeedFromBackend();
    });
    const unlistenIndex = listen("launcher:index-ready", () => void loadApps());
    // Close the startup race where the background index finishes after the first
    // getLauncherApps call but before this event listener is registered.
    void unlistenIndex.then(() => {
      if (!disposed) void loadApps();
    });
    void appWindow
      .onFocusChanged(({ payload: focused }) => {
        // Native file sheets steal focus; suppress blur?hide while they are open (ZTools pattern).
        if (!focused && armed && !pendingRef.current && !isBlurHideSuppressed()) {
          void hidePreservingSession();
          return;
        }
        if (focused && modeRef.current === "search") {
          window.requestAnimationFrame(focusSearchInput);
        }
      })
      .then((unlisten) => {
        unlistenBlur = unlisten;
      });

    prepareForOpen();
    return () => {
      disposed = true;
      window.clearTimeout(armTimer);
      void unlistenOpen.then((unlisten) => unlisten());
      void unlistenShortcutHide.then((unlisten) => unlisten());
      void unlistenClipboard.then((unlisten) => unlisten());
      void unlistenIndex.then((unlisten) => unlisten());
      unlistenBlur?.();
    };
  }, [
    applyClipboardSeedFromBackend,
    clearClipboardChipLocal,
    focusSearchInputAtEnd,
    hidePreservingSession,
    isTauri,
    loadApps,
    markClipboardChipHidden,
    openBuiltinApp,
    refreshDisabledBuiltins,
  ]);

  // Rust owns matching and ranking. A short debounce avoids one IPC + SQLite lookup per
  // keystroke, while cancellation prevents stale responses from replacing current results.
  const normalizedQuery = query.trim();
  const liveNormalizedQuery = normalizedQuery;
  useEffect(() => {
    if (!isTauri || !normalizedQuery) {
      setMatchedSearchApps([]);
      return;
    }

    let cancelled = false;
    const timer = window.setTimeout(() => {
      void api
        .searchMainPanelApps(normalizedQuery, MAX_SEARCH_RESULTS)
        .then((matches) => {
          if (cancelled) return;
          const contributionById = new Map(
            builtinAppsRef.current.map((app) => [app.id, app])
          );
          const results: SearchAppEntry[] = [];

          for (const match of matches) {
            if (match.source === "launcher") {
              if (match.app) {
                results.push({ key: `search:${match.app.id}`, kind: "app", app: match.app });
              }
              continue;
            }

            const app = contributionById.get(match.id);
            if (app) {
              results.push({ key: `builtin:${app.id}`, kind: "builtin", app });
            }
          }

          setMatchedSearchApps(results);
        })
        .catch((searchError) => {
          if (!cancelled) {
            setError(errorMessage(searchError, "搜索应用失败"));
          }
        });
    }, SEARCH_QUERY_DEBOUNCE_MS);

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [isTauri, launcherIndexRevision, normalizedQuery, searchIndexRevision]);

  const recentSource = useMemo<RecentEntry[]>(() => {
    const appsById = new Map(apps.map((app) => [app.id, app]));
    const entries: RecentEntry[] = [];

    for (const usage of usageItems) {
      if (!usage.last_used_at && usage.use_count <= 0) continue;

      if (usage.id.startsWith(BUILTIN_USAGE_PREFIX)) {
        const builtinId = usage.id.slice(BUILTIN_USAGE_PREFIX.length);
        if (disabledBuiltinIds.has(builtinId)) continue;
        const app = getBuiltinApp(builtinId);
        if (!app) continue;
        entries.push({
          key: `recent:builtin:${app.id}`,
          kind: "builtin",
          app,
          last_used_at: usage.last_used_at ?? null,
          use_count: usage.use_count,
        });
        continue;
      }

      if (usage.id.startsWith(PLUGIN_USAGE_PREFIX)) {
        const runtimeAppId = usage.id.slice(PLUGIN_USAGE_PREFIX.length);
        if (disabledBuiltinIds.has(runtimeAppId)) continue;
        const app = getBuiltinApp(runtimeAppId);
        if (!app) continue;
        entries.push({
          key: `recent:plugin:${app.id}`,
          kind: "builtin",
          app,
          last_used_at: usage.last_used_at ?? null,
          use_count: usage.use_count,
        });
        continue;
      }

      const app = appsById.get(usage.id);
      if (!app) continue;
      entries.push({
        key: `recent:${app.id}`,
        kind: "app",
        app,
        last_used_at: usage.last_used_at ?? null,
        use_count: usage.use_count,
      });
    }

    if (entries.length > 0) {
      return entries;
    }

    return apps.map((app) => ({
      key: `recent:${app.id}`,
      kind: "app" as const,
      app,
      last_used_at: app.last_used_at ?? null,
      use_count: app.use_count,
    }));
  }, [apps, disabledBuiltinIds, usageItems]);
  const pinnedApps = useMemo(() => apps.filter((app) => app.pinned), [apps]);
  // Pinned apps have their own row — keep them out of "最近使用".
  const recentWithoutPinned = useMemo(
    () =>
      recentSource.filter(
        (entry) => entry.kind === "builtin" || !entry.app.pinned
      ),
    [recentSource]
  );
  const visibleRecentApps = recentExpanded
    ? recentWithoutPinned
    : recentWithoutPinned.slice(0, RECENT_COLLAPSED_COUNT);
  const visiblePinnedApps = pinnedExpanded
    ? pinnedApps
    : pinnedApps.slice(0, PINNED_COLLAPSED_COUNT);

  const visibleSearchApps = searchExpanded
    ? matchedSearchApps
    : matchedSearchApps.slice(0, SEARCH_COLLAPSED_COUNT);

  const quickActionUsageById = useMemo(() => {
    const map = new Map<
      string,
      { last_used_at?: string | null; use_count: number }
    >();
    for (const usage of usageItems) {
      if (!usage.id.startsWith("action:")) continue;
      map.set(usage.id, {
        last_used_at: usage.last_used_at,
        use_count: usage.use_count,
      });
    }
    return map;
  }, [usageItems]);

  const liveQuickActionQuery = useMemo(
    () => resolveQuickActionQuery(liveNormalizedQuery, clipboardSeedForActions),
    [clipboardSeedForActions, liveNormalizedQuery]
  );
  const quickActionQuery = useMemo(
    () => resolveQuickActionQuery(normalizedQuery, clipboardSeedForActions),
    [clipboardSeedForActions, normalizedQuery]
  );
  const liveQuickActionInput = useMemo(
    () => resolveQuickActionInput(liveNormalizedQuery, clipboardSeedForActions),
    [clipboardSeedForActions, liveNormalizedQuery]
  );
  const quickActionInput = useMemo(
    () => resolveQuickActionInput(normalizedQuery, clipboardSeedForActions),
    [clipboardSeedForActions, normalizedQuery]
  );

  // Recommendations only when there is real input (text / image), never on the empty home.
  const visibleQuickActions = useMemo(() => {
    if (quickActionInput.kind === "none") return [];
    return listVisibleQuickActions(quickActionInput, quickActionUsageById);
  }, [quickActionInput, quickActionUsageById]);

  const showSearchLayout =
    Boolean(normalizedQuery) || quickActionInput.kind !== "none";

  const selectionRows = useMemo<MainPanelSelection[][]>(() => {
    if (showSearchLayout) {
      const appSelections = visibleSearchApps.map((entry) =>
        entry.kind === "builtin"
          ? { key: entry.key, kind: "builtin" as const, app: entry.app }
          : { key: entry.key, kind: "app" as const, app: entry.app }
      );
      const actionSelections = visibleQuickActions.map((action) => ({
        key: `action:${action.id}`,
        kind: "action" as const,
        action,
      }));
      return [
        ...chunkSelections(appSelections),
        ...chunkSelections(actionSelections),
      ];
    }

    const recentSelections = visibleRecentApps.map((entry) =>
      entry.kind === "builtin"
        ? { key: entry.key, kind: "builtin" as const, app: entry.app }
        : { key: entry.key, kind: "app" as const, app: entry.app }
    );
    const pinnedSelections = visiblePinnedApps.map((app) => ({
      key: `pinned:${app.id}`,
      kind: "app" as const,
      app,
    }));
    return [
      ...chunkSelections(pinnedSelections),
      ...chunkSelections(recentSelections),
    ];
  }, [
    showSearchLayout,
    visiblePinnedApps,
    visibleQuickActions,
    visibleRecentApps,
    visibleSearchApps,
  ]);

  const selections = useMemo(() => selectionRows.flat(), [selectionRows]);
  const selectedKeyRef = useRef<string | null>(selectedKey);
  selectedKeyRef.current = selectedKey;
  const selectionRowsRef = useRef(selectionRows);
  selectionRowsRef.current = selectionRows;
  const selectedSelection = selections.find((selection) => selection.key === selectedKey);
  const activeApp = activeAppId ? getBuiltinApp(activeAppId) : undefined;

  // Include the first result because Rust matches arrive after synchronous quick actions.
  // When an app/plugin becomes the leading result, selection must move to it automatically.
  const firstSelectionKey = selections[0]?.key ?? "";
  const searchContextKey = `${showSearchLayout ? 1 : 0}|${normalizedQuery}|${quickActionInput.kind}|${firstSelectionKey}`;
  const prevSearchContextKeyRef = useRef<string | null>(null);

  useEffect(() => {
    setSearchExpanded(false);
  }, [normalizedQuery]);

  // Typing / pasting new input should always land on the first result.
  useLayoutEffect(() => {
    if (mode !== "search") return;
    if (selections.length === 0) {
      selectedKeyRef.current = null;
      setSelectedKey(null);
      prevSearchContextKeyRef.current = searchContextKey;
      return;
    }
    const contextChanged = prevSearchContextKeyRef.current !== searchContextKey;
    prevSearchContextKeyRef.current = searchContextKey;
    if (contextChanged) {
      selectedKeyRef.current = selections[0].key;
      setSelectedKey(selections[0].key);
    }
  }, [mode, searchContextKey, selections]);

  // If the highlighted item disappears (collapse/filter) without a context change, recover.
  // Depend only on `selections` so arrow-key updates do not re-enter this effect.
  useEffect(() => {
    if (mode !== "search") return;
    if (selections.length === 0) {
      selectedKeyRef.current = null;
      setSelectedKey(null);
      return;
    }
    setSelectedKey((current) => {
      if (current && selections.some((selection) => selection.key === current)) {
        return current;
      }
      const next = selections[0].key;
      selectedKeyRef.current = next;
      return next;
    });
  }, [mode, selections]);

  // After plugin -> search: apply SEARCH_WIDTH + measured height in one shot before paint.
  useLayoutEffect(() => {
    if (mode !== "search" || !needsSearchSizeRef.current || !isTauri) return;
    needsSearchSizeRef.current = false;
    const height = contentRef.current
      ? Math.ceil(contentRef.current.scrollHeight)
      : SEARCH_FALLBACK_HEIGHT;
    void api.setMainPanelSize(SEARCH_WIDTH, height);
  }, [isTauri, mode, activeAppId]);

  // ResizeObserver alone tracks content height. Avoid depending on query/lists ?
  // re-subscribing + native set_size on every keystroke is the main input lag source.
  useEffect(() => {
    const content = contentRef.current;
    if (!content || !isTauri || mode !== "search") return;
    let frame = 0;
    let debounceTimer = 0;
    let lastHeight = -1;
    let pendingHeight = -1;
    const flushResize = () => {
      if (pendingHeight < 0) return;
      const nextHeight = pendingHeight;
      pendingHeight = -1;
      void api.setMainPanelSize(null, nextHeight);
    };
    const resize = () => {
      window.cancelAnimationFrame(frame);
      frame = window.requestAnimationFrame(() => {
        const nextHeight = Math.ceil(content.scrollHeight);
        // Ignore 1px flicker from selection/paint so arrow keys don't trigger native resize.
        if (lastHeight >= 0 && Math.abs(nextHeight - lastHeight) <= 1) return;
        if (nextHeight === lastHeight) return;
        lastHeight = nextHeight;
        pendingHeight = nextHeight;
        window.clearTimeout(debounceTimer);
        debounceTimer = window.setTimeout(flushResize, MAIN_PANEL_RESIZE_DEBOUNCE_MS);
      });
    };
    const observer = new ResizeObserver(resize);
    observer.observe(content);
    resize();
    return () => {
      observer.disconnect();
      window.cancelAnimationFrame(frame);
      window.clearTimeout(debounceTimer);
      // Do not flush pending resize on teardown (avoids racing the next mode's size).
    };
  }, [isTauri, mode, openRevision]);

  const executeSelection = useCallback(
    async (selection: MainPanelSelection | undefined) => {
      if (!selection || pendingRef.current) return;
      setError(null);

      if (selection.kind === "builtin") {
        openBuiltinApp(selection.app.id);
        return;
      }

      if (selection.kind === "action") {
        const validationError = selection.action.validate?.(liveQuickActionQuery) ?? null;
        if (validationError) {
          setError(validationError);
          return;
        }
        pendingRef.current = selection.key;
        setPendingKey(selection.key);
        try {
          const usageId = quickActionUsageId(selection.action.id);
          if (isTauri) {
            const now = new Date().toISOString();
            setUsageItems((current) => {
              const existing = current.find((item) => item.id === usageId);
              const nextItem = {
                id: usageId,
                pinned: existing?.pinned ?? false,
                last_used_at: now,
                use_count: (existing?.use_count ?? 0) + 1,
              };
              return [nextItem, ...current.filter((item) => item.id !== usageId)];
            });
            void api
              .recordLauncherUsage(usageId)
              .then(() => api.getLauncherUsage())
              .then(setUsageItems)
              .catch(console.error);
          }
          await selection.action.run({
            query: liveQuickActionQuery,
            input: liveQuickActionInput,
            openApp: openBuiltinApp,
            hideAndReset: hideAndResetMainPanel,
          });
        } catch (executeError) {
          setError(errorMessage(executeError, "操作没有完成"));
        } finally {
          pendingRef.current = null;
          setPendingKey(null);
        }
        return;
      }

      pendingRef.current = selection.key;
      setPendingKey(selection.key);
      try {
        if (!isTauri) return;
        await api.launchIndexedApp(selection.app.id);
        await hideAndResetMainPanel();
        void loadApps();
      } catch (executeError) {
        setError(errorMessage(executeError, "操作没有完成"));
      } finally {
        pendingRef.current = null;
        setPendingKey(null);
      }
    },
    [
      hideAndResetMainPanel,
      isTauri,
      liveQuickActionInput,
      liveQuickActionQuery,
      loadApps,
      openBuiltinApp,
    ]
  );

  const clearClipboardChip = () => {
    clipboardChipRef.current = null;
    setClipboardChip(null);
    setError(null);
    dismissClipboardSeed();
  };

  const clearSearchQuery = () => {
    queryRef.current = "";
    setQuery("");
    selectedKeyRef.current = null;
    setSelectedKey(null);
    setError(null);
    dismissClipboardSeed();
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    // IME: Enter confirms a candidate; arrows move the candidate list. keyCode 229 covers
    // browsers that clear isComposing before the confirming Enter keydown fires.
    if (
      imeComposingRef.current ||
      event.nativeEvent.isComposing ||
      event.keyCode === 229
    ) {
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      if (event.currentTarget.value) {
        clearSearchQuery();
        return;
      }
      if (clipboardChipRef.current) {
        clearClipboardChip();
        return;
      }
      void hideAndResetMainPanel();
      return;
    }
    if (event.key === "Backspace") {
      // Empty query + embedded clipboard chip → clear the chip instead of doing nothing.
      if (!event.currentTarget.value && clipboardChipRef.current) {
        event.preventDefault();
        clearClipboardChip();
      }
      return;
    }
    if (["ArrowDown", "ArrowRight", "ArrowUp", "ArrowLeft"].includes(event.key)) {
      event.preventDefault();
      // Read/write a ref so key-repeat can advance multiple steps before React re-renders.
      const next = moveGridSelection(
        selectionRowsRef.current,
        selectedKeyRef.current,
        event.key
      );
      if (next && next.key !== selectedKeyRef.current) {
        selectedKeyRef.current = next.key;
        setSelectedKey(next.key);
      }
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      const key = selectedKeyRef.current;
      const selection =
        (key && selectionRowsRef.current.flat().find((item) => item.key === key)) ||
        selectedSelection;
      void executeSelection(selection);
    }
  };

  useEffect(() => {
    if (mode !== "app") return;
    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopPropagation();
      backToSearch();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [backToSearch, mode]);

  const togglePinned = async (app: LauncherApp) => {
    if (pendingKey) return;
    const nextPinned = !app.pinned;
    setApps((current) =>
      current.map((item) => (item.id === app.id ? { ...item, pinned: nextPinned } : item))
    );
    if (!isTauri) return;
    try {
      await api.setLauncherAppPinned(app.id, nextPinned);
    } catch (pinError) {
      setApps((current) =>
        current.map((item) => (item.id === app.id ? { ...item, pinned: app.pinned } : item))
      );
      setError(errorMessage(pinError, "无法更新固定状态"));
    }
  };

  const keepSearchFocused = () => {
    inputRef.current?.focus({ preventScroll: true });
  };

  const navigationValue = useMemo(
    () => ({
      openApp: openBuiltinApp,
      backToSearch,
    }),
    [backToSearch, openBuiltinApp]
  );

  const appWindowHeight = resolveRectDimension(
    activeApp?.rect?.height,
    window.screen.availHeight,
    DEFAULT_APP_HEIGHT
  );
  const appBodyHeight = Math.max(0, appWindowHeight - APP_CHROME_HEIGHT);
  const showApp = Boolean(mode === "app" && activeApp);
  const flushApp = Boolean(
    activeApp && (FLUSH_APP_IDS.has(activeApp.id) || activeApp.source === "plugin")
  );
  const fillAppHeight = Boolean(
    activeApp && (activeApp.ui.type !== "react" || FILL_HEIGHT_APP_IDS.has(activeApp.id))
  );

  const activeAppNode =
    showApp && activeApp ? (
      activeApp.ui.type === "react" ? (
        <activeApp.ui.component
          onBack={backToSearch}
          openCreateOnMount={activeApp.id === "snippets" ? openCreateSnippet : undefined}
          initialTranslateText={
            activeApp.id === "translate" ? initialTranslateText : undefined
          }
        />
      ) : (
        <PluginAppHost
          pluginId={activeApp.pluginId}
          appId={activeApp.ui.localAppId}
          params={activeAppParams}
        />
      )
    ) : null;

  return (
    <BuiltinAppNavigationProvider value={navigationValue}>
      <main className={cn("main-panel-page", showApp && "main-panel-page--app")}>
        <div
          ref={contentRef}
          className="main-panel-surface"
          onMouseDownCapture={(event) => {
            if (!showApp) {
              if (event.target === inputRef.current) return;
              const target = event.target;
              if (
                target instanceof Element &&
                target.closest(
                  ".main-panel-search, button, input, textarea, a, [data-no-drag]"
                )
              ) {
                return;
              }
              event.preventDefault();
              keepSearchFocused();
            }
          }}
        >
          <header className="main-panel-search" onMouseDown={beginMainPanelDrag}>
            {showApp && activeApp ? (
              <>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-lg"
                  className="main-panel-back-button"
                  data-no-drag
                  aria-label="返回搜索"
                  title="返回搜索 (Esc)"
                  onClick={backToSearch}
                >
                  <ArrowLeft />
                </Button>
                <div className="main-panel-app-bar-title">{activeApp.name}</div>
                <div className="main-panel-search-spacer" aria-hidden="true" />
                <div className="main-panel-app-bar-icon" aria-hidden="true">
                  <AppIconView icon={activeApp.icon} className="main-panel-app-bar-icon-glyph" />
                </div>
              </>
            ) : (
              <>
                <div className="main-panel-search-field">
                  {clipboardChip ? (
                    <div className="main-panel-clipboard-chip" data-no-drag>
                      {clipboardChip.kind === "text" ? (
                        <>
                          <FollowTooltip content={clipboardChip.fullText}>
                            <span className="main-panel-clipboard-chip-label">
                              {clipboardChip.label}
                            </span>
                          </FollowTooltip>
                          <Button
                            type="button"
                            variant="ghost"
                            size="icon-sm"
                            className="main-panel-clipboard-chip-edit"
                            onClick={() => {
                              setQuery(clipboardChip.fullText);
                              queryRef.current = clipboardChip.fullText;
                              setClipboardChip(null);
                              clipboardChipRef.current = null;
                              dismissClipboardSeed();
                              window.requestAnimationFrame(focusSearchInputAtEnd);
                            }}
                          >
                            <Pencil className="size-3.5" />
                          </Button>
                        </>
                      ) : clipboardChip.kind === "file" ? (
                        <FollowTooltip
                          content={formatClipboardFilesPreview(clipboardChip.paths)}
                        >
                          <span className="main-panel-clipboard-chip-file">
                            <ClipboardFileGlyph paths={clipboardChip.paths} size="chip" />
                          </span>
                        </FollowTooltip>
                      ) : (
                        <FollowTooltip content="图片">
                          <img
                            src={clipboardChip.imageUrl}
                            alt=""
                            className="main-panel-clipboard-chip-image"
                            width={clipboardChip.imageWidth ?? undefined}
                            height={clipboardChip.imageHeight ?? undefined}
                          />
                        </FollowTooltip>
                      )}
                    </div>
                  ) : null}
                  <div ref={inputWrapperRef} className="main-panel-input-wrapper">
                    <span ref={inputMeasureRef} className="main-panel-input-measure" aria-hidden>
                      {query}
                    </span>
                    {!query ? (
                      <span className="main-panel-input-placeholder" aria-hidden>
                        {clipboardChip ? "搜索匹配" : "搜索应用或输入命令"}
                      </span>
                    ) : null}
                    <input
                      ref={inputRef}
                      value={query}
                      className="main-panel-input"
                      placeholder=""
                      autoComplete="off"
                      spellCheck={false}
                      aria-label="搜索应用或输入命令"
                      onPaste={(event) => {
                        void handleManualPaste(event);
                      }}
                      onChange={(event) => {
                        const nextQuery = event.target.value;
                        const clearedByUser = Boolean(queryRef.current) && !nextQuery;
                        queryRef.current = nextQuery;
                        setQuery(nextQuery);
                        setError(null);
                        if (clearedByUser) dismissClipboardSeed();
                      }}
                      onCompositionStart={() => {
                        imeComposingRef.current = true;
                      }}
                      onCompositionEnd={() => {
                        // Confirming Enter often arrives in the same turn as compositionend;
                        // clear on the next frame so it does not execute the selected app.
                        window.requestAnimationFrame(() => {
                          imeComposingRef.current = false;
                        });
                      }}
                      onKeyDown={handleKeyDown}
                    />
                  </div>
                </div>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-lg"
                  className="main-panel-logo-button"
                  data-no-drag
                  aria-label="打开设置"
                  title="打开设置"
                  onClick={() => openBuiltinApp("settings")}
                >
                  <img src="/favicon.png" alt="" className="main-panel-logo" />
                </Button>
              </>
            )}
          </header>

          <div className="main-panel-content">
            {showApp && activeAppNode ? (
              <div
                className={cn(
                  "main-panel-app-host",
                  fillAppHeight && !flushApp && "main-panel-app-host--padded"
                )}
                style={{ height: appBodyHeight }}
              >
                {fillAppHeight ? (
                  activeAppNode
                ) : (
                  <ScrollArea className="h-full">
                    <div className="box-border p-4 px-5 pb-5">{activeAppNode}</div>
                  </ScrollArea>
                )}
              </div>
            ) : loading ? (
              <LauncherLoading />
            ) : showSearchLayout ? (
              <SearchResults
                apps={visibleSearchApps}
                totalAppCount={matchedSearchApps.length}
                quickActions={visibleQuickActions}
                expanded={searchExpanded}
                query={quickActionQuery}
                selectedKey={selectedKey}
                pendingKey={pendingKey}
                onToggleExpanded={() => setSearchExpanded((current) => !current)}
                onExecute={(selection) => void executeSelection(selection)}
                onTogglePinned={(app) => void togglePinned(app)}
              />
            ) : (
              <DefaultApps
                recentApps={visibleRecentApps}
                recentTotal={recentWithoutPinned.length}
                pinnedApps={visiblePinnedApps}
                pinnedTotal={pinnedApps.length}
                recentExpanded={recentExpanded}
                pinnedExpanded={pinnedExpanded}
                selectedKey={selectedKey}
                pendingKey={pendingKey}
                onToggleRecent={() => setRecentExpanded((current) => !current)}
                onTogglePinnedSection={() => setPinnedExpanded((current) => !current)}
                onExecute={(selection) => void executeSelection(selection)}
                onTogglePinned={(app) => void togglePinned(app)}
              />
            )}
            {!showApp && error ? (
              <p className="main-panel-error" role="alert">
                {error}
              </p>
            ) : null}
          </div>
        </div>
      </main>
      <ReminderDialog event={reminder} onDismiss={() => setReminder(null)} />
      <Toaster position="top-center" richColors toastOptions={appToastOptions} />
    </BuiltinAppNavigationProvider>
  );
}

function DefaultApps({
  recentApps,
  recentTotal,
  pinnedApps,
  pinnedTotal,
  recentExpanded,
  pinnedExpanded,
  selectedKey,
  pendingKey,
  onToggleRecent,
  onTogglePinnedSection,
  onExecute,
  onTogglePinned,
}: {
  recentApps: RecentEntry[];
  recentTotal: number;
  pinnedApps: LauncherApp[];
  pinnedTotal: number;
  recentExpanded: boolean;
  pinnedExpanded: boolean;
  selectedKey: string | null;
  pendingKey: string | null;
  onToggleRecent: () => void;
  onTogglePinnedSection: () => void;
  onExecute: (selection: MainPanelSelection) => void;
  onTogglePinned: (app: LauncherApp) => void;
}) {
  return (
    <div className="main-panel-sections">
      {pinnedTotal > 0 ? (
        <LauncherSection
          id="launcher-pinned-title"
          title="已固定"
          total={pinnedTotal}
          collapsedCount={PINNED_COLLAPSED_COUNT}
          expanded={pinnedExpanded}
          onToggle={onTogglePinnedSection}
        >
          <div className="main-panel-app-grid">
            {pinnedApps.map((app) => {
              const key = `pinned:${app.id}`;
              return (
                <AppTile
                  key={key}
                  selectionKey={key}
                  app={app}
                  selected={selectedKey === key}
                  pending={pendingKey === key}
                  onExecute={() => onExecute({ key, kind: "app", app })}
                  onTogglePinned={() => onTogglePinned(app)}
                />
              );
            })}
          </div>
        </LauncherSection>
      ) : null}

      {recentTotal > 0 ? (
        <LauncherSection
          id="launcher-recent-title"
          title="最近使用"
          total={recentTotal}
          collapsedCount={RECENT_COLLAPSED_COUNT}
          expanded={recentExpanded}
          onToggle={onToggleRecent}
        >
          <div className="main-panel-app-grid">
            {recentApps.map((entry) => {
              if (entry.kind === "builtin") {
                return (
                  <BuiltinTile
                    key={entry.key}
                    selectionKey={entry.key}
                    app={entry.app}
                    selected={selectedKey === entry.key}
                    onExecute={() =>
                      onExecute({ key: entry.key, kind: "builtin", app: entry.app })
                    }
                  />
                );
              }
              return (
                <AppTile
                  key={entry.key}
                  selectionKey={entry.key}
                  app={entry.app}
                  selected={selectedKey === entry.key}
                  pending={pendingKey === entry.key}
                  onExecute={() => onExecute({ key: entry.key, kind: "app", app: entry.app })}
                  onTogglePinned={() => onTogglePinned(entry.app)}
                />
              );
            })}
          </div>
        </LauncherSection>
      ) : null}
    </div>
  );
}

function SearchResults({
  apps,
  totalAppCount,
  quickActions,
  expanded,
  query,
  selectedKey,
  pendingKey,
  onToggleExpanded,
  onExecute,
  onTogglePinned,
}: {
  apps: SearchAppEntry[];
  totalAppCount: number;
  quickActions: QuickAction[];
  expanded: boolean;
  query: string;
  selectedKey: string | null;
  pendingKey: string | null;
  onToggleExpanded: () => void;
  onExecute: (selection: MainPanelSelection) => void;
  onTogglePinned: (app: LauncherApp) => void;
}) {
  const hasResults = totalAppCount > 0 || quickActions.length > 0;

  return (
    <div className="main-panel-sections">
      {!hasResults ? (
        <p className="main-panel-empty" role="status">
          无匹配结果
        </p>
      ) : null}

      {totalAppCount > 0 ? (
        <LauncherSection
          id="launcher-app-results"
          title="应用与插件"
          total={totalAppCount}
          collapsedCount={SEARCH_COLLAPSED_COUNT}
          expanded={expanded}
          onToggle={onToggleExpanded}
        >
          <div className="main-panel-app-grid">
            {apps.map((entry) => {
              if (entry.kind === "builtin") {
                return (
                  <BuiltinTile
                    key={entry.key}
                    selectionKey={entry.key}
                    app={entry.app}
                    selected={selectedKey === entry.key}
                    onExecute={() =>
                      onExecute({ key: entry.key, kind: "builtin", app: entry.app })
                    }
                  />
                );
              }
              return (
                <AppTile
                  key={entry.key}
                  selectionKey={entry.key}
                  app={entry.app}
                  selected={selectedKey === entry.key}
                  pending={pendingKey === entry.key}
                  onExecute={() =>
                    onExecute({ key: entry.key, kind: "app", app: entry.app })
                  }
                  onTogglePinned={() => onTogglePinned(entry.app)}
                />
              );
            })}
          </div>
        </LauncherSection>
      ) : null}

      {quickActions.length > 0 ? (
        <LauncherSection id="launcher-action-results" title="推荐操作">
          <QuickActionTiles
            actions={quickActions}
            query={query}
            selectedKey={selectedKey}
            pendingKey={pendingKey}
            onExecute={onExecute}
          />
        </LauncherSection>
      ) : null}
    </div>
  );
}

function QuickActionTiles({
  actions,
  query,
  selectedKey,
  pendingKey,
  onExecute,
}: {
  actions: QuickAction[];
  query: string;
  selectedKey: string | null;
  pendingKey: string | null;
  onExecute: (selection: MainPanelSelection) => void;
}) {
  return (
    <div className="main-panel-app-grid">
      {actions.map((action) => {
        const key = `action:${action.id}`;
        const validationError = action.validate?.(query) ?? null;
        const pending = pendingKey === key;
        return (
          <button
            key={key}
            type="button"
            className="main-panel-action-tile"
            data-selected={selectedKey === key || undefined}
            disabled={Boolean(validationError) || pending}
            title={validationError ?? action.title?.(query) ?? action.name}
            onClick={() => onExecute({ key, kind: "action", action })}
          >
            <span className="main-panel-action-icon">
              {pending ? (
                <LoaderCircle className="main-panel-inline-loader" aria-hidden="true" />
              ) : (
                <AppIconView icon={action.icon} />
              )}
            </span>
            <span>{validationError ? "内容无效" : action.name}</span>
          </button>
        );
      })}
    </div>
  );
}

function LauncherSection({
  id,
  title,
  total,
  collapsedCount,
  expanded,
  onToggle,
  children,
}: {
  id: string;
  title: string;
  total?: number;
  collapsedCount?: number;
  expanded?: boolean;
  onToggle?: () => void;
  children: React.ReactNode;
}) {
  const expandable = Boolean(total && collapsedCount && total > collapsedCount && onToggle);
  return (
    <section className="main-panel-section" aria-labelledby={id}>
      <div className="main-panel-section-heading">
        <h2 id={id}>{title}</h2>
        {expandable ? (
          <button type="button" className="main-panel-expand-button" onClick={onToggle}>
            {expanded ? "收起" : `展开 (${total})`}
          </button>
        ) : null}
      </div>
      {children}
    </section>
  );
}

function BuiltinTile({
  selectionKey,
  app,
  selected,
  onExecute,
}: {
  selectionKey: string;
  app: BuiltinApp;
  selected: boolean;
  onExecute: () => void;
}) {
  return (
    <div
      className="main-panel-app-tile-wrap"
      data-selected={selected || undefined}
      data-selection-key={selectionKey}
    >
      {app.source === "plugin" ? (
        <span className="main-panel-plugin-badge" title="插件">
          插件
        </span>
      ) : null}
      <button
        type="button"
        className="main-panel-app-tile"
        title={app.name}
        onClick={onExecute}
      >
        <span className="main-panel-builtin-icon">
          <AppIconView icon={app.icon} />
        </span>
        <span>{app.name}</span>
      </button>
    </div>
  );
}

function AppTile({
  selectionKey,
  app,
  selected,
  pending,
  onExecute,
  onTogglePinned,
}: {
  selectionKey: string;
  app: LauncherApp;
  selected: boolean;
  pending: boolean;
  onExecute: () => void;
  onTogglePinned: () => void;
}) {
  return (
    <div
      className="main-panel-app-tile-wrap"
      data-selected={selected || undefined}
      data-selection-key={selectionKey}
    >
      <button
        type="button"
        className="main-panel-app-tile"
        disabled={pending}
        title={app.name}
        onClick={onExecute}
      >
        {pending ? (
          <LoaderCircle className="main-panel-app-loader" aria-hidden="true" />
        ) : (
          <AppIcon
            name={app.name}
            iconDataUrl={app.icon_data_url}
            className="size-8"
            fallback="application"
            fallbackClassName="bg-muted text-muted-foreground"
          />
        )}
        <span>{app.name}</span>
      </button>
      <Button
        type="button"
        variant="ghost"
        size="icon-xs"
        className="main-panel-pin-button"
        aria-label={app.pinned ? `取消固定 ${app.name}` : `固定 ${app.name}`}
        title={app.pinned ? "取消固定" : "固定"}
        onClick={onTogglePinned}
      >
        {app.pinned ? <PinOff /> : <Pin />}
      </Button>
    </div>
  );
}

function LauncherLoading() {
  return (
    <div className="main-panel-loading" aria-label="正在读取本机应用">
      <div className="main-panel-section-heading">
        <Skeleton className="h-4 w-16" />
      </div>
      <div className="main-panel-app-grid">
        {Array.from({ length: GRID_COLUMNS }, (_, index) => (
          <div key={index} className="main-panel-loading-tile">
            <Skeleton className="size-8" />
            <Skeleton className="h-3 w-12" />
          </div>
        ))}
      </div>
    </div>
  );
}

function chunkSelections(selections: MainPanelSelection[]): MainPanelSelection[][] {
  const rows: MainPanelSelection[][] = [];
  for (let index = 0; index < selections.length; index += GRID_COLUMNS) {
    rows.push(selections.slice(index, index + GRID_COLUMNS));
  }
  return rows;
}

function moveGridSelection(
  rows: MainPanelSelection[][],
  selectedKey: string | null,
  direction: string
): MainPanelSelection | undefined {
  if (rows.length === 0) return undefined;

  let rowIndex = 0;
  let columnIndex = 0;
  for (let index = 0; index < rows.length; index += 1) {
    const match = rows[index].findIndex((selection) => selection.key === selectedKey);
    if (match >= 0) {
      rowIndex = index;
      columnIndex = match;
      break;
    }
  }

  if (direction === "ArrowDown" || direction === "ArrowUp") {
    const delta = direction === "ArrowDown" ? 1 : -1;
    const nextRowIndex = rowIndex + delta;
    if (nextRowIndex >= 0 && nextRowIndex < rows.length) {
      rowIndex = nextRowIndex;
      columnIndex = Math.min(columnIndex, rows[rowIndex].length - 1);
    }
  } else if (direction === "ArrowRight") {
    columnIndex = Math.min(columnIndex + 1, rows[rowIndex].length - 1);
  } else if (direction === "ArrowLeft") {
    columnIndex = Math.max(columnIndex - 1, 0);
  }

  return rows[rowIndex][columnIndex];
}

function resolveRectDimension(
  value: AppRectValue | undefined,
  available: number,
  fallback: number
): number {
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value === "string" && value.trim().endsWith("%")) {
    const percent = Number.parseFloat(value);
    if (Number.isFinite(percent)) return (available * percent) / 100;
  }
  return fallback;
}

function isTauriRuntime(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

async function hideMainPanel() {
  if (!isTauriRuntime()) return;
  await getCurrentWindow().hide();
}

function errorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  if (error && typeof error === "object") {
    const message = (error as { message?: unknown }).message;
    if (typeof message === "string" && message.trim()) return message;
  }
  return fallback;
}
