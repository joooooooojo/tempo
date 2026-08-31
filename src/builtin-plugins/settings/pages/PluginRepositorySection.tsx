import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import {
  AlertTriangle,
  ArrowDown,
  ArrowUp,
  FileKey,
  FolderOpen,
  GitBranch,
  KeyRound,
  MoreVertical,
  PackageSearch,
  Pencil,
  Plus,
  RefreshCw,
  Search,
  ShieldAlert,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { api } from "@/lib/api";
import { openNativeFileDialog } from "@/lib/nativeFileDialog";
import type {
  AddPluginRepositoryInput,
  CreatePluginRepositoryTemplateInput,
  PluginRepository,
  RepositoryCatalogPlugin,
  RepositoryCredentialProfile,
  RepositoryIssue,
  RepositoryOperation,
  RepositoryTrustChallenge,
  SaveRepositoryCredentialInput,
} from "@/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogPanel,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import { cn } from "@/lib/utils";

const OPERATION_EVENT = "plugin-repository-operation-progress";
const AUTO_SYNC_AFTER_MS = 6 * 60 * 60 * 1000;
const AUTO_SYNCED_REPOSITORIES = new Set<string>();
const NOTIFIED_OPERATION_IDS = new Set<string>();

function rememberNotifiedOperation(operationId: string): boolean {
  if (NOTIFIED_OPERATION_IDS.has(operationId)) return false;
  NOTIFIED_OPERATION_IDS.add(operationId);
  if (NOTIFIED_OPERATION_IDS.size > 200) {
    const oldest = NOTIFIED_OPERATION_IDS.values().next().value;
    if (oldest) NOTIFIED_OPERATION_IDS.delete(oldest);
  }
  return true;
}

function markTerminalOperationsNotified(operations: RepositoryOperation[]) {
  for (const operation of operations) {
    if (operation.status === "completed" || operation.status === "failed") {
      NOTIFIED_OPERATION_IDS.add(operation.operationId);
    }
  }
}

const EMPTY_REPOSITORY_DRAFT: AddPluginRepositoryInput = {
  url: "",
  gitRef: "HEAD",
  indexPath: "tempo-plugin-repository.json",
  displayName: "",
  authenticationMode: "anonymous",
  credentialId: null,
  allowInsecureTransport: false,
  allowInsecureCredentials: false,
};

const EMPTY_TEMPLATE_DRAFT: CreatePluginRepositoryTemplateInput = {
  parentPath: "",
  folderName: "",
  description: "",
  homepage: "",
};

const EMPTY_CREDENTIAL_DRAFT: SaveRepositoryCredentialInput = {
  id: null,
  displayName: "",
  scopeUrl: "",
  authKind: "http-token",
  username: "",
  sshPrivateKeyPath: "",
  secret: "",
  persist: true,
};

const REPOSITORY_AUTH_OPTIONS = [
  { value: "anonymous", label: "匿名访问" },
  { value: "credential", label: "使用凭证" },
];

const CREDENTIAL_KIND_OPTIONS = [
  { value: "http-token", label: "PAT / 密码" },
  { value: "ssh-agent", label: "SSH Agent" },
  { value: "ssh-key", label: "SSH 私钥" },
];

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function shortCommit(commit: string | null | undefined) {
  return commit ? commit.slice(0, 8) : null;
}

function formatSyncTime(value: string | null | undefined) {
  if (!value) return "尚未同步";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "已同步";
  return new Intl.DateTimeFormat("zh-CN", {
    month: "numeric",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

function credentialKindLabel(kind: string) {
  switch (kind) {
    case "http-token":
      return "PAT / 密码";
    case "ssh-agent":
      return "SSH Agent";
    case "ssh-key":
      return "SSH 私钥";
    default:
      return kind;
  }
}

function repositoryOrigin(raw: string) {
  let value = raw.trim();
  if (!value) return null;
  if (!value.includes("://")) {
    const scp = value.match(/^(?:([^@/:]+)@)?([^/:]+):(.+)$/);
    if (!scp) return null;
    value = `ssh://${scp[1] ? `${scp[1]}@` : ""}${scp[2]}/${scp[3]}`;
  }
  try {
    const parsed = new URL(value);
    const scheme = parsed.protocol.slice(0, -1).toLowerCase();
    if (!["http", "https", "ssh"].includes(scheme) || !parsed.hostname) return null;
    const port =
      parsed.port || (scheme === "http" ? "80" : scheme === "https" ? "443" : "22");
    return `${scheme}://${parsed.hostname.toLowerCase()}:${port}`;
  } catch {
    return null;
  }
}

function repositoryStatus(repository: PluginRepository) {
  if (repository.lastError) return { label: "同步失败", variant: "destructive" as const };
  if (repository.credentialStatus === "missing" || repository.credentialStatus === "session-missing") {
    return { label: "缺少凭证", variant: "destructive" as const };
  }
  if (!repository.enabled) return { label: "已停用", variant: "secondary" as const };
  if (!repository.snapshotCommit) return { label: "待同步", variant: "outline" as const };
  return { label: "可用", variant: "secondary" as const };
}

function operationIsActive(operation: RepositoryOperation | undefined) {
  return operation?.status === "queued" || operation?.status === "running";
}

function pluginActionLabel(plugin: RepositoryCatalogPlugin) {
  switch (plugin.action) {
    case "update":
      return "更新";
    case "installed":
      return "已安装";
    case "pending":
      return "待切换";
    case "conflict":
      return "版本冲突";
    case "unavailable":
      return "不可安装";
    default:
      return "安装";
  }
}

export function PluginRepositorySection() {
  const [repositories, setRepositories] = useState<PluginRepository[]>([]);
  const [credentials, setCredentials] = useState<RepositoryCredentialProfile[]>([]);
  const [catalog, setCatalog] = useState<RepositoryCatalogPlugin[]>([]);
  const [operations, setOperations] = useState<RepositoryOperation[]>([]);
  const [loading, setLoading] = useState(true);
  const [query, setQuery] = useState("");
  const [addOpen, setAddOpen] = useState(false);
  const [sourcesOpen, setSourcesOpen] = useState(false);
  const [templateOpen, setTemplateOpen] = useState(false);
  const [editingRepositoryId, setEditingRepositoryId] = useState<string | null>(null);
  const [credentialsOpen, setCredentialsOpen] = useState(false);
  const [issuesFor, setIssuesFor] = useState<PluginRepository | null>(null);
  const [issues, setIssues] = useState<RepositoryIssue[]>([]);
  const [issuesLoading, setIssuesLoading] = useState(false);
  const [trustPrompt, setTrustPrompt] = useState<{
    repository: PluginRepository;
    challenge: RepositoryTrustChallenge;
    syncAfter: boolean;
  } | null>(null);
  const [trustingConnection, setTrustingConnection] = useState(false);
  const [repositoryDraft, setRepositoryDraft] = useState<AddPluginRepositoryInput>({
    ...EMPTY_REPOSITORY_DRAFT,
  });
  const [credentialDraft, setCredentialDraft] = useState<SaveRepositoryCredentialInput>({
    ...EMPTY_CREDENTIAL_DRAFT,
  });
  const [savingRepository, setSavingRepository] = useState(false);
  const [creatingTemplate, setCreatingTemplate] = useState(false);
  const [templateDraft, setTemplateDraft] = useState<CreatePluginRepositoryTemplateInput>({
    ...EMPTY_TEMPLATE_DRAFT,
  });
  const [savingCredential, setSavingCredential] = useState(false);
  const [moreOpenId, setMoreOpenId] = useState<string | null>(null);
  const catalogRequest = useRef(0);
  const queryRef = useRef(query);
  const operationsRef = useRef(operations);
  queryRef.current = query;
  operationsRef.current = operations;

  const refreshRepositories = useCallback(async () => {
    const [nextRepositories, nextCredentials, nextOperations] = await Promise.all([
      api.listPluginRepositories(),
      api.listPluginRepositoryCredentials(),
      api.listPluginRepositoryOperations(),
    ]);
    markTerminalOperationsNotified(nextOperations);
    setRepositories(nextRepositories);
    setCredentials(nextCredentials);
    setOperations(nextOperations);
    return nextRepositories;
  }, []);

  const refreshCatalog = useCallback(async (search = "") => {
    const request = ++catalogRequest.current;
    const next = await api.searchRepositoryPlugins(search);
    if (request === catalogRequest.current) setCatalog(next);
  }, []);

  const refreshAll = useCallback(async () => {
    await Promise.all([refreshRepositories(), refreshCatalog(query)]);
  }, [query, refreshCatalog, refreshRepositories]);

  useEffect(() => {
    let cancelled = false;
    Promise.all([refreshRepositories(), refreshCatalog("")])
      .then(([nextRepositories]) => {
        for (const repository of nextRepositories) {
          const lastSuccess = repository.lastSuccessAt
            ? new Date(repository.lastSuccessAt).getTime()
            : 0;
          const stale = !Number.isFinite(lastSuccess) || Date.now() - lastSuccess > AUTO_SYNC_AFTER_MS;
          if (!repository.enabled || !stale || AUTO_SYNCED_REPOSITORIES.has(repository.id)) {
            continue;
          }
          AUTO_SYNCED_REPOSITORIES.add(repository.id);
          void api
            .syncPluginRepository(repository.id)
            .then(() => refreshRepositories())
            .catch(console.error);
        }
      })
      .catch((error) => {
        if (!cancelled) toast.error(errorText(error));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [refreshCatalog, refreshRepositories]);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      void refreshCatalog(query).catch((error) => toast.error(errorText(error)));
    }, 180);
    return () => window.clearTimeout(timer);
  }, [query, refreshCatalog]);

  const handleOperation = useCallback(
    (operation: RepositoryOperation) => {
      setOperations((current) => {
        const index = current.findIndex((item) => item.operationId === operation.operationId);
        if (index < 0) return [operation, ...current];
        const next = [...current];
        next[index] = operation;
        return next;
      });
      if (operation.status !== "completed" && operation.status !== "failed") return;
      if (!rememberNotifiedOperation(operation.operationId)) return;
      const toastId = `plugin-repo-op:${operation.operationId}`;
      if (operation.status === "completed") {
        toast.success(operation.message, { id: toastId });
      } else {
        toast.error(operation.error || operation.message, { id: toastId });
      }
      void Promise.all([refreshRepositories(), refreshCatalog(queryRef.current)]).catch(
        console.error,
      );
    },
    [refreshCatalog, refreshRepositories],
  );

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void listen<RepositoryOperation>(OPERATION_EVENT, (event) => {
      handleOperation(event.payload);
    }).then((dispose) => {
      if (cancelled) dispose();
      else unlisten = dispose;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [handleOperation]);

  const hasActiveOperation = operations.some(operationIsActive);
  useEffect(() => {
    if (!hasActiveOperation) return;
    const timer = window.setInterval(() => {
      void api.listPluginRepositoryOperations().then((next) => {
        const activeIds = new Set(
          operationsRef.current.filter(operationIsActive).map((item) => item.operationId),
        );
        for (const operation of next) {
          if (
            (operation.status === "completed" || operation.status === "failed") &&
            !activeIds.has(operation.operationId)
          ) {
            NOTIFIED_OPERATION_IDS.add(operation.operationId);
          }
        }
        setOperations(next);
        next.forEach(handleOperation);
      });
    }, 1000);
    return () => window.clearInterval(timer);
  }, [handleOperation, hasActiveOperation]);

  const repositoryOperations = useMemo(() => {
    const map = new Map<string, RepositoryOperation>();
    for (const operation of operations) {
      if (operation.kind === "sync" && !map.has(operation.repositoryId)) {
        map.set(operation.repositoryId, operation);
      }
    }
    return map;
  }, [operations]);

  const pluginOperations = useMemo(() => {
    const map = new Map<string, RepositoryOperation>();
    for (const operation of operations) {
      if (operation.kind === "install" && operation.pluginId && !map.has(operation.pluginId)) {
        map.set(operation.pluginId, operation);
      }
    }
    return map;
  }, [operations]);

  const isHttpDraft = repositoryDraft.url.trim().toLowerCase().startsWith("http://");
  const isSshDraft =
    repositoryDraft.url.trim().toLowerCase().startsWith("ssh://") ||
    (!repositoryDraft.url.includes("://") && repositoryDraft.url.includes(":"));
  const draftOrigin = repositoryOrigin(repositoryDraft.url);
  const matchingCredentials = credentials.filter(
    (credential) => !draftOrigin || credential.scopeOrigin === draftOrigin,
  );

  const openAddRepository = () => {
    setEditingRepositoryId(null);
    setRepositoryDraft({ ...EMPTY_REPOSITORY_DRAFT });
    setAddOpen(true);
  };

  const openCreateTemplate = () => {
    setTemplateDraft({ ...EMPTY_TEMPLATE_DRAFT });
    setTemplateOpen(true);
  };

  const chooseTemplateDirectory = async () => {
    const path = await openNativeFileDialog({
      directory: true,
      multiple: false,
      title: "选择插件仓库保存位置",
    });
    if (typeof path !== "string") return;
    setTemplateDraft((current) => ({ ...current, parentPath: path }));
  };

  const createTemplate = async () => {
    if (!templateDraft.parentPath.trim()) {
      return;
    }
    setCreatingTemplate(true);
    try {
      const created = await api.createPluginRepositoryFromTemplate({
        parentPath: templateDraft.parentPath.trim(),
        folderName: templateDraft.folderName?.trim() || null,
        description: templateDraft.description?.trim() || null,
        homepage: templateDraft.homepage?.trim() || null,
      });
      setTemplateOpen(false);
      toast.success(
        created.gitInitialized
          ? "已在本机创建插件仓库并初始化 Git。推送到远程后，用「添加源」填写 Git 地址。"
          : "已在本机创建插件仓库。推送到远程后，用「添加源」填写 Git 地址。",
      );
    } catch (error) {
      toast.error(errorText(error));
    } finally {
      setCreatingTemplate(false);
    }
  };

  const editRepository = (repository: PluginRepository) => {
    setMoreOpenId(null);
    setEditingRepositoryId(repository.id);
    setRepositoryDraft({
      url: repository.url,
      gitRef: repository.gitRef,
      indexPath: repository.indexPath,
      displayName: repository.displayName || "",
      authenticationMode:
        repository.authenticationMode === "credential" ? "credential" : "anonymous",
      credentialId: repository.credentialId || null,
      allowInsecureTransport: repository.allowInsecureTransport,
      allowInsecureCredentials: repository.allowInsecureCredentials,
    });
    setAddOpen(true);
  };

  const saveRepository = async () => {
    if (!repositoryDraft.url.trim()) return;
    setSavingRepository(true);
    try {
      const input = {
        ...repositoryDraft,
        url: repositoryDraft.url.trim(),
        gitRef: repositoryDraft.gitRef.trim() || "HEAD",
        indexPath: repositoryDraft.indexPath.trim() || "tempo-plugin-repository.json",
        displayName: repositoryDraft.displayName?.trim() || null,
        credentialId:
          repositoryDraft.authenticationMode === "credential"
            ? repositoryDraft.credentialId || null
            : null,
        allowInsecureTransport: isHttpDraft && repositoryDraft.allowInsecureTransport,
        allowInsecureCredentials:
          isHttpDraft &&
          repositoryDraft.authenticationMode === "credential" &&
          repositoryDraft.allowInsecureCredentials,
      };
      const previous = repositories.find((item) => item.id === editingRepositoryId);
      const repository = editingRepositoryId
        ? await api.updatePluginRepository({ ...input, repositoryId: editingRepositoryId })
        : await api.addPluginRepository(input);
      const shouldSync =
        !previous ||
        previous.url !== repository.url ||
        previous.gitRef !== repository.gitRef ||
        previous.indexPath !== repository.indexPath ||
        previous.authenticationMode !== repository.authenticationMode ||
        previous.credentialId !== repository.credentialId ||
        previous.allowInsecureTransport !== repository.allowInsecureTransport ||
        previous.allowInsecureCredentials !== repository.allowInsecureCredentials;
      setAddOpen(false);
      setEditingRepositoryId(null);
      setRepositoryDraft({ ...EMPTY_REPOSITORY_DRAFT });
      await refreshRepositories();
      if (shouldSync) {
        await testConnection(repository, true);
      } else {
        toast.success("仓库已更新");
      }
    } catch (error) {
      toast.error(errorText(error));
    } finally {
      setSavingRepository(false);
    }
  };

  const syncAllRepositories = async () => {
    try {
      await api.syncAllPluginRepositories();
      await refreshRepositories();
    } catch (error) {
      toast.error(errorText(error));
    }
  };

  const moveRepository = async (repositoryId: string, offset: -1 | 1) => {
    const index = repositories.findIndex((repository) => repository.id === repositoryId);
    const target = index + offset;
    if (index < 0 || target < 0 || target >= repositories.length) return;
    const previous = repositories;
    const next = [...repositories];
    [next[index], next[target]] = [next[target], next[index]];
    setRepositories(next.map((repository, priority) => ({ ...repository, priority })));
    try {
      await api.reorderPluginRepositories(next.map((repository) => repository.id));
      await refreshCatalog(query);
    } catch (error) {
      setRepositories(previous);
      toast.error(errorText(error));
    }
  };

  const syncRepository = async (repository: PluginRepository) => {
    try {
      const started = await api.syncPluginRepository(repository.id);
      const next: RepositoryOperation = {
        operationId: started.operationId,
        kind: "sync",
        repositoryId: repository.id,
        pluginId: null,
        status: "queued",
        phase: "queued",
        message: "等待处理",
        transferredBytes: 0,
        totalItems: null,
        completedItems: 0,
        error: null,
      };
      setOperations((current) => [
        next,
        ...current.filter((item) => item.operationId !== started.operationId),
      ]);
    } catch (error) {
      toast.error(errorText(error));
    }
  };

  const testConnection = async (
    repository: PluginRepository,
    syncAfter: boolean,
    showSuccess = false,
  ) => {
    try {
      const result = await api.testPluginRepositoryConnection(repository.id);
      if (result.status === "ok") {
        await refreshRepositories();
        if (syncAfter) {
          await syncRepository({ ...repository, connectionVerifiedAt: new Date().toISOString() });
          toast.success("连接已验证，正在同步");
        } else if (showSuccess) {
          toast.success(result.message);
        }
        return;
      }
      if (result.status === "trust-required" && result.challenge) {
        setTrustPrompt({ repository, challenge: result.challenge, syncAfter });
        return;
      }
      toast.error(`连接测试失败：${result.message}`);
    } catch (error) {
      toast.error(errorText(error));
    }
  };

  const syncOrTestRepository = async (repository: PluginRepository) => {
    if (repository.connectionVerifiedAt) {
      await syncRepository(repository);
    } else {
      await testConnection(repository, true);
    }
  };

  const trustConnection = async () => {
    if (!trustPrompt) return;
    setTrustingConnection(true);
    const { repository, challenge, syncAfter } = trustPrompt;
    try {
      const input = {
        confirmationNonce: challenge.confirmationNonce,
        host: challenge.host,
        port: challenge.port,
        fingerprintSha256: challenge.fingerprintSha256,
      };
      if (challenge.kind === "ssh-host-key") {
        await api.trustPluginRepositorySshHostKey(input);
      } else {
        await api.trustPluginRepositoryTlsCertificate(input);
      }
      setTrustPrompt(null);
      await testConnection(repository, syncAfter, !syncAfter);
    } catch (error) {
      toast.error(errorText(error));
    } finally {
      setTrustingConnection(false);
    }
  };

  const setRepositoryEnabled = async (repository: PluginRepository, enabled: boolean) => {
    const previous = repositories;
    setRepositories((current) =>
      current.map((item) => (item.id === repository.id ? { ...item, enabled } : item)),
    );
    try {
      await api.setPluginRepositoryEnabled(repository.id, enabled);
      await Promise.all([refreshRepositories(), refreshCatalog(query)]);
    } catch (error) {
      setRepositories(previous);
      toast.error(errorText(error));
    }
  };

  const removeRepository = async (repository: PluginRepository) => {
    if (!window.confirm(`删除插件仓库“${repository.name}”？已安装的插件不会被删除。`)) return;
    try {
      await api.removePluginRepository(repository.id);
      setMoreOpenId(null);
      await refreshAll();
      toast.success("仓库已删除");
    } catch (error) {
      toast.error(errorText(error));
    }
  };

  const showIssues = async (repository: PluginRepository) => {
    setMoreOpenId(null);
    setIssuesFor(repository);
    setIssues([]);
    setIssuesLoading(true);
    try {
      setIssues(await api.listPluginRepositoryIssues(repository.id));
    } catch (error) {
      toast.error(errorText(error));
    } finally {
      setIssuesLoading(false);
    }
  };

  const installPlugin = async (plugin: RepositoryCatalogPlugin) => {
    try {
      const started = await api.installRepositoryPlugin(
        plugin.repositoryId,
        plugin.id,
        plugin.sourceCommit,
      );
      const next: RepositoryOperation = {
        operationId: started.operationId,
        kind: "install",
        repositoryId: plugin.repositoryId,
        pluginId: plugin.id,
        status: "queued",
        phase: "queued",
        message: "等待处理",
        transferredBytes: 0,
        totalItems: null,
        completedItems: 0,
        error: null,
      };
      setOperations((current) => [
        next,
        ...current.filter((item) => item.operationId !== started.operationId),
      ]);
    } catch (error) {
      toast.error(errorText(error));
    }
  };

  const startNewCredential = () => {
    setCredentialDraft({
      ...EMPTY_CREDENTIAL_DRAFT,
      scopeUrl: repositoryDraft.url.trim(),
    });
  };

  const editCredential = (credential: RepositoryCredentialProfile) => {
    setCredentialDraft({
      id: credential.id,
      displayName: credential.displayName,
      scopeUrl: credential.scopeOrigin,
      authKind:
        credential.authKind === "ssh-agent" || credential.authKind === "ssh-key"
          ? credential.authKind
          : "http-token",
      username: credential.username || "",
      sshPrivateKeyPath: credential.sshPrivateKeyPath || "",
      secret: "",
      persist: credential.secretStorage !== "session",
    });
  };

  const saveCredential = async () => {
    setSavingCredential(true);
    try {
      const saved = await api.savePluginRepositoryCredential({
        ...credentialDraft,
        displayName: credentialDraft.displayName.trim(),
        scopeUrl: credentialDraft.scopeUrl.trim(),
        username: credentialDraft.username?.trim() || null,
        sshPrivateKeyPath: credentialDraft.sshPrivateKeyPath?.trim() || null,
        secret: credentialDraft.secret || null,
      });
      setCredentialDraft({ ...EMPTY_CREDENTIAL_DRAFT });
      await refreshRepositories();
      setRepositoryDraft((current) => ({
        ...current,
        authenticationMode: "credential",
        credentialId: current.credentialId || saved.id,
      }));
      toast.success("凭证已保存");
    } catch (error) {
      toast.error(errorText(error));
    } finally {
      setCredentialDraft((current) => ({ ...current, secret: "" }));
      setSavingCredential(false);
    }
  };

  const deleteCredential = async (credential: RepositoryCredentialProfile) => {
    const suffix = credential.referencedRepositoryCount
      ? `，${credential.referencedRepositoryCount} 个仓库将缺少凭证`
      : "";
    if (!window.confirm(`删除凭证“${credential.displayName}”${suffix}？`)) return;
    try {
      await api.deletePluginRepositoryCredential(credential.id);
      if (credentialDraft.id === credential.id) {
        setCredentialDraft({ ...EMPTY_CREDENTIAL_DRAFT });
      }
      await refreshRepositories();
      toast.success("凭证已删除");
    } catch (error) {
      toast.error(errorText(error));
    }
  };

  const choosePrivateKey = async () => {
    try {
      const selected = await openNativeFileDialog({
        directory: false,
        multiple: false,
        title: "选择 SSH 私钥",
      });
      if (!selected || Array.isArray(selected)) return;
      setCredentialDraft((current) => ({ ...current, sshPrivateKeyPath: selected }));
    } catch (error) {
      toast.error(errorText(error));
    }
  };

  const credentialNeedsSecret =
    credentialDraft.authKind === "http-token" || credentialDraft.authKind === "ssh-key";

  if (loading) {
    return (
      <div className="plugin-repository-loading">
        <Spinner />
        <span>正在读取插件仓库</span>
      </div>
    );
  }

  return (
    <div className="settings-panel-stack plugin-repositories">
      <Dialog open={sourcesOpen} onOpenChange={setSourcesOpen}>
        <DialogPanel className="plugin-repository-dialog plugin-repository-manager-dialog sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>仓库源</DialogTitle>
            <DialogDescription>
              从 Git 仓库读取已构建的插件，安装后仍需单独信任。
            </DialogDescription>
          </DialogHeader>
          <DialogContent className="plugin-repository-manager-dialog__content">
            <div className="plugin-repository-manager-toolbar">
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  if (credentials[0]) editCredential(credentials[0]);
                  else startNewCredential();
                  setCredentialsOpen(true);
                }}
              >
                <KeyRound data-icon="inline-start" />
                凭证
              </Button>
              <Button
                variant="outline"
                size="sm"
                disabled={repositories.length === 0 || hasActiveOperation}
                onClick={() => void syncAllRepositories()}
              >
                <RefreshCw data-icon="inline-start" />
                全部同步
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={openCreateTemplate}
              >
                <FolderOpen data-icon="inline-start" />
                从模板创建
              </Button>
              <Button size="sm" onClick={openAddRepository}>
                <Plus data-icon="inline-start" />
                添加源
              </Button>
            </div>

            {repositories.length === 0 ? (
              <Card>
                <Empty className="min-h-36">
                  <EmptyHeader>
                    <EmptyMedia variant="icon">
                      <GitBranch />
                    </EmptyMedia>
                    <EmptyTitle>还没有插件仓库</EmptyTitle>
                    <EmptyDescription>
                      在本机从官方模板创建仓库，或添加已有的公开 / 企业 Git 源。安装后仍需单独信任插件。
                    </EmptyDescription>
                  </EmptyHeader>
                  <EmptyContent className="flex-row flex-wrap justify-center">
                    <Button size="sm" onClick={openCreateTemplate}>
                      <FolderOpen data-icon="inline-start" />
                      从模板创建
                    </Button>
                    <Button size="sm" variant="outline" onClick={openAddRepository}>
                      <Plus data-icon="inline-start" />
                      添加源
                    </Button>
                  </EmptyContent>
                </Empty>
              </Card>
            ) : (
              <Card>
                <ul className="plugin-repository-list">
                  {repositories.map((repository) => {
                    const status = repositoryStatus(repository);
                    const operation = repositoryOperations.get(repository.id);
                    const syncing = operationIsActive(operation);
                    const meta = syncing
                      ? operation?.message
                      : `${repository.validPluginCount} 个插件 · ${formatSyncTime(repository.lastSuccessAt)}`;
                    return (
                      <li
                        key={repository.id}
                        className={cn(
                          "plugin-repository-source",
                          !repository.enabled && "plugin-repository-source--disabled",
                        )}
                      >
                        <span className="plugin-repository-source__icon" aria-hidden="true">
                          {syncing ? <Spinner /> : <GitBranch className="size-4" />}
                        </span>
                        <div className="plugin-repository-source__identity">
                          <div className="plugin-repository-source__title-row">
                            <span className="plugin-repository-source__name">{repository.name}</span>
                            <Badge variant={status.variant}>{status.label}</Badge>
                            {repository.transport === "http" ? (
                              <Badge variant="destructive">HTTP</Badge>
                            ) : null}
                          </div>
                          <span className="plugin-repository-source__url" title={repository.url}>
                            {repository.url}
                          </span>
                          <span
                            className={cn(
                              "plugin-repository-source__meta",
                              repository.lastError && !syncing && "text-destructive",
                            )}
                            title={repository.lastError || undefined}
                          >
                            {syncing ? meta : repository.lastError || meta}
                            {repository.snapshotCommit
                              ? ` · ${shortCommit(repository.snapshotCommit)}`
                              : ""}
                          </span>
                        </div>
                        <div className="plugin-repository-source__controls">
                          <Button
                            variant="ghost"
                            size="icon-sm"
                            disabled={syncing || !repository.enabled}
                            aria-label={`同步 ${repository.name}`}
                            title="同步"
                            onClick={() => void syncOrTestRepository(repository)}
                          >
                            {syncing ? <Spinner /> : <RefreshCw />}
                          </Button>
                          <Switch
                            checked={repository.enabled}
                            onCheckedChange={(enabled) =>
                              void setRepositoryEnabled(repository, enabled)
                            }
                          />
                          <Popover
                            open={moreOpenId === repository.id}
                            onOpenChange={(open) => setMoreOpenId(open ? repository.id : null)}
                          >
                            <PopoverTrigger asChild>
                              <Button
                                variant="ghost"
                                size="icon-sm"
                                aria-label={`管理 ${repository.name}`}
                                title="更多"
                              >
                                <MoreVertical />
                              </Button>
                            </PopoverTrigger>
                            <PopoverContent align="end" side="bottom" className="w-48 gap-0.5 p-1">
                              <button
                                type="button"
                                className="plugin-more__item"
                                disabled={syncing || !repository.enabled}
                                onClick={() => {
                                  setMoreOpenId(null);
                                  void testConnection(repository, false, true);
                                }}
                              >
                                <ShieldCheck className="size-3.5 text-muted-foreground" />
                                测试连接
                              </button>
                              <button
                                type="button"
                                className="plugin-more__item"
                                disabled={syncing}
                                onClick={() => editRepository(repository)}
                              >
                                <Pencil className="size-3.5 text-muted-foreground" />
                                编辑仓库
                              </button>
                              <button
                                type="button"
                                className="plugin-more__item"
                                disabled={repository.priority <= 0}
                                onClick={() => {
                                  setMoreOpenId(null);
                                  void moveRepository(repository.id, -1);
                                }}
                              >
                                <ArrowUp className="size-3.5 text-muted-foreground" />
                                上移
                              </button>
                              <button
                                type="button"
                                className="plugin-more__item"
                                disabled={repository.priority >= repositories.length - 1}
                                onClick={() => {
                                  setMoreOpenId(null);
                                  void moveRepository(repository.id, 1);
                                }}
                              >
                                <ArrowDown className="size-3.5 text-muted-foreground" />
                                下移
                              </button>
                              <button
                                type="button"
                                className="plugin-more__item"
                                disabled={repository.issueCount === 0}
                                onClick={() => void showIssues(repository)}
                              >
                                <AlertTriangle className="size-3.5 text-muted-foreground" />
                                隔离问题（{repository.issueCount}）
                              </button>
                              <button
                                type="button"
                                className="plugin-more__item plugin-more__item--danger"
                                disabled={syncing}
                                onClick={() => void removeRepository(repository)}
                              >
                                <Trash2 className="size-3.5" />
                                删除仓库
                              </button>
                            </PopoverContent>
                          </Popover>
                        </div>
                      </li>
                    );
                  })}
                </ul>
              </Card>
            )}
          </DialogContent>
        </DialogPanel>

        <section className="settings-section">
          <div className="plugin-catalog-heading">
            <h2 className="settings-section__title">可用插件</h2>
            <div className="plugin-catalog-heading__actions">
              <Button variant="outline" size="sm" onClick={openCreateTemplate}>
                <FolderOpen data-icon="inline-start" />
                从模板创建
              </Button>
              <DialogTrigger asChild>
                <Button variant="outline" size="sm">
                  <GitBranch data-icon="inline-start" />
                  仓库源
                </Button>
              </DialogTrigger>
              <div className="plugin-catalog-search">
                <Search className="plugin-catalog-search__icon" aria-hidden="true" />
                <Input
                  value={query}
                  placeholder="搜索名称、发布者或 ID"
                  aria-label="搜索仓库插件"
                  onChange={(event) => setQuery(event.target.value)}
                />
              </div>
            </div>
          </div>

          {catalog.length === 0 ? (
          <Card>
            <Empty className="min-h-36">
              <EmptyHeader>
                <EmptyMedia variant="icon">
                  <PackageSearch />
                </EmptyMedia>
                <EmptyTitle>{query ? "没有匹配的插件" : "仓库目录为空"}</EmptyTitle>
                <EmptyDescription>
                  {query
                    ? "换个关键词试试。"
                    : repositories.length === 0
                      ? "在本机从官方模板创建仓库，推送到 Git 后再添加为源；也可以直接添加已有 Git 地址。"
                      : "同步仓库后，可安装的插件会显示在这里。"}
                </EmptyDescription>
              </EmptyHeader>
              {!query && repositories.length === 0 ? (
                <EmptyContent className="flex-row flex-wrap justify-center">
                  <Button size="sm" onClick={openCreateTemplate}>
                    <FolderOpen data-icon="inline-start" />
                    从模板创建
                  </Button>
                  <Button size="sm" variant="outline" onClick={openAddRepository}>
                    <Plus data-icon="inline-start" />
                    添加源
                  </Button>
                </EmptyContent>
              ) : null}
            </Empty>
          </Card>
        ) : (
          <Card>
            <ul className="repository-plugin-list">
              {catalog.map((plugin) => {
                const operation = pluginOperations.get(plugin.id);
                const installing = operationIsActive(operation);
                const disabled =
                  installing ||
                  !plugin.compatible ||
                  !["install", "update"].includes(plugin.action);
                const versionMeta = plugin.installedVersion
                  ? `v${plugin.installedVersion} → v${plugin.version}`
                  : `v${plugin.version}`;
                return (
                  <li key={`${plugin.repositoryId}:${plugin.id}`} className="repository-plugin-item">
                    <span className="repository-plugin-item__icon" aria-hidden="true">
                      {plugin.iconUrl ? (
                        <img src={plugin.iconUrl} alt="" draggable={false} />
                      ) : (
                        <PackageSearch className="size-4 opacity-70" />
                      )}
                    </span>
                    <div className="repository-plugin-item__identity">
                      <div className="repository-plugin-item__title-row">
                        <span className="repository-plugin-item__name">{plugin.name}</span>
                        <span className="repository-plugin-item__version">{versionMeta}</span>
                      </div>
                      <span className="repository-plugin-item__description">
                        {plugin.description || plugin.id}
                      </span>
                      <span
                        className={cn(
                          "repository-plugin-item__meta",
                          (!plugin.compatible || plugin.action === "conflict") &&
                            "text-destructive",
                        )}
                      >
                        {plugin.incompatibleReason ||
                          (plugin.action === "conflict"
                            ? "同版本内容与已安装包不同"
                            : `${plugin.publisher || "未声明发布者"} · ${plugin.repositoryName}`)}
                      </span>
                    </div>
                    <Button
                      size="sm"
                      variant={plugin.action === "install" ? "default" : "outline"}
                      disabled={disabled}
                      title={plugin.incompatibleReason || undefined}
                      onClick={() => void installPlugin(plugin)}
                    >
                      {installing ? <Spinner data-icon="inline-start" /> : null}
                      {installing ? operation?.message || "安装中" : pluginActionLabel(plugin)}
                    </Button>
                  </li>
                );
              })}
            </ul>
          </Card>
          )}
        </section>
      </Dialog>

      <Dialog
        open={addOpen}
        onOpenChange={(open) => {
          setAddOpen(open);
          if (!open) {
            setEditingRepositoryId(null);
            setRepositoryDraft({ ...EMPTY_REPOSITORY_DRAFT });
          }
        }}
      >
        <DialogPanel showOverlay={!sourcesOpen} className="plugin-repository-dialog sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>{editingRepositoryId ? "编辑插件仓库" : "添加插件仓库"}</DialogTitle>
            <DialogDescription>
              Tempo 会拉取目标 Git 快照，并只从索引列出的 dist 目录安装插件。
            </DialogDescription>
          </DialogHeader>
          <DialogContent>
            <FieldGroup className="gap-4">
              <Field>
                <FieldLabel htmlFor="plugin-repository-url">Git 地址</FieldLabel>
                <Input
                  id="plugin-repository-url"
                  value={repositoryDraft.url}
                  placeholder="https://git.example.com/team/plugins.git"
                  autoComplete="off"
                  onChange={(event) =>
                    setRepositoryDraft((current) => ({ ...current, url: event.target.value }))
                  }
                />
                <FieldDescription>
                  支持 HTTPS、确认后的 HTTP、SSH 和 git@host:path 地址。
                </FieldDescription>
              </Field>
              <div className="plugin-repository-dialog__columns">
                <Field>
                  <FieldLabel htmlFor="plugin-repository-name">显示名称</FieldLabel>
                  <Input
                    id="plugin-repository-name"
                    value={repositoryDraft.displayName || ""}
                    placeholder="同步后使用仓库名称"
                    onChange={(event) =>
                      setRepositoryDraft((current) => ({
                        ...current,
                        displayName: event.target.value,
                      }))
                    }
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor="plugin-repository-ref">分支或 ref</FieldLabel>
                  <Input
                    id="plugin-repository-ref"
                    value={repositoryDraft.gitRef}
                    placeholder="HEAD"
                    autoComplete="off"
                    onChange={(event) =>
                      setRepositoryDraft((current) => ({
                        ...current,
                        gitRef: event.target.value,
                      }))
                    }
                  />
                </Field>
              </div>
              <Field>
                <FieldLabel htmlFor="plugin-repository-index">索引文件</FieldLabel>
                <Input
                  id="plugin-repository-index"
                  value={repositoryDraft.indexPath}
                  autoComplete="off"
                  onChange={(event) =>
                    setRepositoryDraft((current) => ({
                      ...current,
                      indexPath: event.target.value,
                    }))
                  }
                />
              </Field>
              <Field>
                <FieldLabel>认证</FieldLabel>
                <Select
                  items={REPOSITORY_AUTH_OPTIONS}
                  value={repositoryDraft.authenticationMode}
                  onValueChange={(value) =>
                    setRepositoryDraft((current) => ({
                      ...current,
                      authenticationMode: value as "anonymous" | "credential",
                      credentialId: value === "anonymous" ? null : current.credentialId,
                    }))
                  }
                >
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent overlayLayer>
                    <SelectGroup>
                      <SelectItem value="anonymous" disabled={isSshDraft}>
                        匿名访问
                      </SelectItem>
                      <SelectItem value="credential">使用凭证</SelectItem>
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </Field>
              {repositoryDraft.authenticationMode === "credential" ? (
                <Field data-invalid={!repositoryDraft.credentialId}>
                  <div className="plugin-repository-credential-label">
                    <FieldLabel>凭证档案</FieldLabel>
                    <button
                      type="button"
                      className="plugin-repository-inline-action"
                      onClick={() => {
                        startNewCredential();
                        setCredentialsOpen(true);
                      }}
                    >
                      管理凭证
                    </button>
                  </div>
                  <Select
                    items={matchingCredentials.map((credential) => ({
                      value: credential.id,
                      label: `${credential.displayName} · ${credential.scopeOrigin}`,
                    }))}
                    value={repositoryDraft.credentialId || ""}
                    onValueChange={(value) =>
                      setRepositoryDraft((current) => ({ ...current, credentialId: value }))
                    }
                  >
                    <SelectTrigger className="w-full" aria-invalid={!repositoryDraft.credentialId}>
                      <SelectValue placeholder="选择与仓库地址匹配的凭证" />
                    </SelectTrigger>
                    <SelectContent searchable searchPlaceholder="搜索凭证" overlayLayer>
                      <SelectGroup>
                        {matchingCredentials.map((credential) => (
                          <SelectItem
                            key={credential.id}
                            value={credential.id}
                            disabled={!credential.available}
                          >
                            {credential.displayName} · {credential.scopeOrigin}
                            {!credential.available ? "（当前不可用）" : ""}
                          </SelectItem>
                        ))}
                      </SelectGroup>
                    </SelectContent>
                  </Select>
                  {draftOrigin && matchingCredentials.length === 0 ? (
                    <FieldDescription>没有适用于 {draftOrigin} 的凭证。</FieldDescription>
                  ) : null}
                </Field>
              ) : null}
              {isHttpDraft ? (
                <div className="plugin-repository-risk">
                  <ShieldAlert className="size-4 shrink-0" aria-hidden="true" />
                  <div className="min-w-0 flex-1">
                    <p>HTTP 连接未加密</p>
                    <span>仓库内容可能在传输中被读取或修改。</span>
                  </div>
                  <Switch
                    checked={repositoryDraft.allowInsecureTransport}
                    aria-label="允许 HTTP 不安全传输"
                    onCheckedChange={(checked) =>
                      setRepositoryDraft((current) => ({
                        ...current,
                        allowInsecureTransport: checked,
                      }))
                    }
                  />
                </div>
              ) : null}
              {isHttpDraft && repositoryDraft.authenticationMode === "credential" ? (
                <div className="plugin-repository-risk">
                  <FileKey className="size-4 shrink-0" aria-hidden="true" />
                  <div className="min-w-0 flex-1">
                    <p>允许通过 HTTP 发送凭证</p>
                    <span>Token 或密码可能被网络中的其他人读取。</span>
                  </div>
                  <Switch
                    checked={repositoryDraft.allowInsecureCredentials}
                    aria-label="允许通过 HTTP 发送凭证"
                    onCheckedChange={(checked) =>
                      setRepositoryDraft((current) => ({
                        ...current,
                        allowInsecureCredentials: checked,
                      }))
                    }
                  />
                </div>
              ) : null}
            </FieldGroup>
          </DialogContent>
          <DialogFooter>
            <Button variant="outline" onClick={() => setAddOpen(false)}>
              取消
            </Button>
            <Button
              disabled={
                savingRepository ||
                !repositoryDraft.url.trim() ||
                (repositoryDraft.authenticationMode === "credential" &&
                  !repositoryDraft.credentialId) ||
                (isSshDraft && repositoryDraft.authenticationMode === "anonymous") ||
                (isHttpDraft && !repositoryDraft.allowInsecureTransport) ||
                (isHttpDraft &&
                  repositoryDraft.authenticationMode === "credential" &&
                  !repositoryDraft.allowInsecureCredentials)
              }
              onClick={() => void saveRepository()}
            >
              {savingRepository ? <Spinner data-icon="inline-start" /> : null}
              {editingRepositoryId ? "保存" : "添加并测试"}
            </Button>
          </DialogFooter>
        </DialogPanel>
      </Dialog>

      <Dialog
        open={templateOpen}
        onOpenChange={(open) => {
          setTemplateOpen(open);
          if (!open) setTemplateDraft({ ...EMPTY_TEMPLATE_DRAFT });
        }}
      >
        <DialogPanel showOverlay={!sourcesOpen} className="plugin-repository-dialog sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>从模板创建仓库</DialogTitle>
            <DialogDescription>
              Tempo 会写入官方模板、自动生成仓库 ID 并初始化 Git。推送到远程后，再用「添加源」填 Git 地址。
            </DialogDescription>
          </DialogHeader>
          <DialogContent>
            <FieldGroup className="gap-4">
              <Field>
                <FieldLabel htmlFor="plugin-repository-template-parent">保存位置</FieldLabel>
                <div className="flex gap-2">
                  <Input
                    id="plugin-repository-template-parent"
                    value={templateDraft.parentPath}
                    placeholder="选择父文件夹"
                    autoComplete="off"
                    onChange={(event) =>
                      setTemplateDraft((current) => ({ ...current, parentPath: event.target.value }))
                    }
                  />
                  <Button
                    type="button"
                    variant="outline"
                    size="icon"
                    aria-label="选择目录"
                    onClick={() => void chooseTemplateDirectory()}
                  >
                    <FolderOpen />
                  </Button>
                </div>
              </Field>
              <Field>
                <FieldLabel htmlFor="plugin-repository-template-folder">文件夹名称</FieldLabel>
                <Input
                  id="plugin-repository-template-folder"
                  value={templateDraft.folderName || ""}
                  placeholder="输入文件夹名称"
                  autoComplete="off"
                  onChange={(event) =>
                    setTemplateDraft((current) => ({ ...current, folderName: event.target.value }))
                  }
                />
              </Field>
              <Field>
                <FieldLabel htmlFor="plugin-repository-template-description">说明</FieldLabel>
                <Input
                  id="plugin-repository-template-description"
                  value={templateDraft.description || ""}
                  placeholder="可选"
                  autoComplete="off"
                  onChange={(event) =>
                    setTemplateDraft((current) => ({
                      ...current,
                      description: event.target.value,
                    }))
                  }
                />
              </Field>
            </FieldGroup>
          </DialogContent>
          <DialogFooter>
            <Button variant="outline" onClick={() => setTemplateOpen(false)}>
              取消
            </Button>
            <Button
              disabled={creatingTemplate || !templateDraft.parentPath.trim()}
              onClick={() => void createTemplate()}
            >
              {creatingTemplate ? <Spinner data-icon="inline-start" /> : <FolderOpen data-icon="inline-start" />}
              创建仓库
            </Button>
          </DialogFooter>
        </DialogPanel>
      </Dialog>

      <Dialog
        open={credentialsOpen}
        onOpenChange={(open) => {
          setCredentialsOpen(open);
          if (!open) setCredentialDraft({ ...EMPTY_CREDENTIAL_DRAFT });
        }}
      >
        <DialogPanel showOverlay={false} className="plugin-repository-dialog sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>仓库凭证</DialogTitle>
            <DialogDescription>
              秘密保存在系统凭证库或当前应用会话中，不会写入仓库地址和数据库。
            </DialogDescription>
          </DialogHeader>
          <DialogContent className="plugin-credential-dialog__content">
            <div className="plugin-credential-layout">
              <div className="plugin-credential-list" aria-label="已保存凭证">
                <div className="plugin-credential-list__header">
                  <span>凭证档案</span>
                  <Button variant="ghost" size="icon-sm" title="新建凭证" onClick={startNewCredential}>
                    <Plus />
                  </Button>
                </div>
                {credentials.length === 0 ? (
                  <p className="plugin-credential-list__empty">暂无凭证</p>
                ) : (
                  credentials.map((credential) => (
                    <button
                      key={credential.id}
                      type="button"
                      className={cn(
                        "plugin-credential-list__item",
                        credentialDraft.id === credential.id &&
                          "plugin-credential-list__item--active",
                      )}
                      onClick={() => editCredential(credential)}
                    >
                      <span>{credential.displayName}</span>
                      <small>
                        {credentialKindLabel(credential.authKind)}
                        {credential.username ? ` · ${credential.username}` : ""}
                      </small>
                    </button>
                  ))
                )}
              </div>

              <FieldGroup className="plugin-credential-form gap-4">
                <div className="plugin-repository-dialog__columns">
                  <Field>
                    <FieldLabel htmlFor="plugin-credential-name">名称</FieldLabel>
                    <Input
                      id="plugin-credential-name"
                      value={credentialDraft.displayName}
                      placeholder="企业 GitLab"
                      onChange={(event) =>
                        setCredentialDraft((current) => ({
                          ...current,
                          displayName: event.target.value,
                        }))
                      }
                    />
                  </Field>
                  <Field>
                    <FieldLabel>类型</FieldLabel>
                    <Select
                      items={CREDENTIAL_KIND_OPTIONS}
                      value={credentialDraft.authKind}
                      onValueChange={(value) =>
                        setCredentialDraft((current) => ({
                          ...current,
                          authKind: value as SaveRepositoryCredentialInput["authKind"],
                          secret: "",
                          sshPrivateKeyPath:
                            value === "ssh-key" ? current.sshPrivateKeyPath : "",
                        }))
                      }
                    >
                      <SelectTrigger className="w-full">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent overlayLayer>
                        <SelectGroup>
                          <SelectItem value="http-token">PAT / 密码</SelectItem>
                          <SelectItem value="ssh-agent">SSH Agent</SelectItem>
                          <SelectItem value="ssh-key">SSH 私钥</SelectItem>
                        </SelectGroup>
                      </SelectContent>
                    </Select>
                  </Field>
                </div>
                <Field>
                  <FieldLabel htmlFor="plugin-credential-scope">适用仓库地址</FieldLabel>
                  <Input
                    id="plugin-credential-scope"
                    value={credentialDraft.scopeUrl}
                    placeholder="https://git.example.com/group/plugins.git"
                    autoComplete="off"
                    onChange={(event) =>
                      setCredentialDraft((current) => ({
                        ...current,
                        scopeUrl: event.target.value,
                      }))
                    }
                  />
                  <FieldDescription>凭证只会用于完全相同的协议、主机和端口。</FieldDescription>
                </Field>
                <Field>
                  <FieldLabel htmlFor="plugin-credential-username">用户名</FieldLabel>
                  <Input
                    id="plugin-credential-username"
                    value={credentialDraft.username || ""}
                    autoComplete="username"
                    onChange={(event) =>
                      setCredentialDraft((current) => ({
                        ...current,
                        username: event.target.value,
                      }))
                    }
                  />
                </Field>
                {credentialDraft.authKind === "ssh-key" ? (
                  <Field>
                    <FieldLabel htmlFor="plugin-credential-key">SSH 私钥</FieldLabel>
                    <div className="plugin-credential-path-control">
                      <Input
                        id="plugin-credential-key"
                        value={credentialDraft.sshPrivateKeyPath || ""}
                        placeholder="选择私钥文件"
                        autoComplete="off"
                        onChange={(event) =>
                          setCredentialDraft((current) => ({
                            ...current,
                            sshPrivateKeyPath: event.target.value,
                          }))
                        }
                      />
                      <Button variant="outline" onClick={() => void choosePrivateKey()}>
                        选择
                      </Button>
                    </div>
                  </Field>
                ) : null}
                {credentialNeedsSecret ? (
                  <Field>
                    <FieldLabel htmlFor="plugin-credential-secret">
                      {credentialDraft.authKind === "http-token" ? "Token 或密码" : "私钥口令"}
                    </FieldLabel>
                    <Input
                      id="plugin-credential-secret"
                      type="password"
                      value={credentialDraft.secret || ""}
                      placeholder={credentialDraft.id ? "留空则保留已保存的秘密" : ""}
                      autoComplete="new-password"
                      onChange={(event) =>
                        setCredentialDraft((current) => ({
                          ...current,
                          secret: event.target.value,
                        }))
                      }
                    />
                  </Field>
                ) : null}
                {credentialNeedsSecret ? (
                  <Field orientation="horizontal" className="plugin-credential-persist">
                    <div className="min-w-0 flex-1">
                      <FieldLabel htmlFor="plugin-credential-persist">保存在系统凭证库</FieldLabel>
                      <FieldDescription>关闭后，秘密仅在当前应用会话中可用。</FieldDescription>
                    </div>
                    <Switch
                      id="plugin-credential-persist"
                      checked={credentialDraft.persist}
                      onCheckedChange={(checked) =>
                        setCredentialDraft((current) => ({ ...current, persist: checked }))
                      }
                    />
                  </Field>
                ) : null}
                <div className="plugin-credential-form__actions">
                  {credentialDraft.id ? (
                    <Button
                      variant="destructive"
                      onClick={() => {
                        const credential = credentials.find(
                          (item) => item.id === credentialDraft.id,
                        );
                        if (credential) void deleteCredential(credential);
                      }}
                    >
                      <Trash2 data-icon="inline-start" />
                      删除
                    </Button>
                  ) : null}
                  <Button
                    className="ml-auto"
                    disabled={
                      savingCredential ||
                      !credentialDraft.displayName.trim() ||
                      !credentialDraft.scopeUrl.trim() ||
                      !credentialDraft.username?.trim() ||
                      (credentialDraft.authKind === "ssh-key" &&
                        !credentialDraft.sshPrivateKeyPath?.trim()) ||
                      (credentialDraft.authKind === "http-token" &&
                        !credentialDraft.id &&
                        !credentialDraft.secret)
                    }
                    onClick={() => void saveCredential()}
                  >
                    {savingCredential ? <Spinner data-icon="inline-start" /> : null}
                    {credentialDraft.id ? "保存修改" : "保存凭证"}
                  </Button>
                </div>
              </FieldGroup>
            </div>
          </DialogContent>
          <DialogFooter>
            <Button variant="outline" onClick={() => setCredentialsOpen(false)}>
              完成
            </Button>
          </DialogFooter>
        </DialogPanel>
      </Dialog>

      <Dialog
        open={Boolean(trustPrompt)}
        onOpenChange={(open) => {
          if (!open && !trustingConnection) setTrustPrompt(null);
        }}
      >
        <DialogPanel showOverlay={false} className="plugin-repository-dialog sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>
              {trustPrompt?.challenge.kind === "ssh-host-key"
                ? "确认 SSH 主机密钥"
                : "确认 TLS 证书"}
            </DialogTitle>
            <DialogDescription>
              请通过仓库管理员或可信渠道核对完整指纹。确认后只信任这个主机和端口。
            </DialogDescription>
          </DialogHeader>
          <DialogContent>
            {trustPrompt ? (
              <FieldGroup className="plugin-repository-trust-details gap-2.5">
                <Field orientation="horizontal">
                  <FieldLabel htmlFor="plugin-repository-trust-host">主机</FieldLabel>
                  <Input
                    id="plugin-repository-trust-host"
                    readOnly
                    value={`${trustPrompt.challenge.host}:${trustPrompt.challenge.port}`}
                  />
                </Field>
                {trustPrompt.challenge.keyType ? (
                  <Field orientation="horizontal">
                    <FieldLabel htmlFor="plugin-repository-trust-key-type">密钥类型</FieldLabel>
                    <Input
                      id="plugin-repository-trust-key-type"
                      readOnly
                      value={trustPrompt.challenge.keyType}
                    />
                  </Field>
                ) : null}
                {trustPrompt.challenge.subject ? (
                  <Field orientation="horizontal">
                    <FieldLabel htmlFor="plugin-repository-trust-subject">证书主体</FieldLabel>
                    <Input
                      id="plugin-repository-trust-subject"
                      readOnly
                      value={trustPrompt.challenge.subject}
                    />
                  </Field>
                ) : null}
                {trustPrompt.challenge.issuer ? (
                  <Field orientation="horizontal">
                    <FieldLabel htmlFor="plugin-repository-trust-issuer">签发者</FieldLabel>
                    <Input
                      id="plugin-repository-trust-issuer"
                      readOnly
                      value={trustPrompt.challenge.issuer}
                    />
                  </Field>
                ) : null}
                <Field orientation="horizontal">
                  <FieldLabel htmlFor="plugin-repository-trust-fingerprint">SHA-256 指纹</FieldLabel>
                  <Input
                    id="plugin-repository-trust-fingerprint"
                    readOnly
                    className="font-mono text-[11px]"
                    value={trustPrompt.challenge.fingerprintSha256}
                  />
                </Field>
              </FieldGroup>
            ) : null}
          </DialogContent>
          <DialogFooter>
            <Button
              variant="outline"
              disabled={trustingConnection}
              onClick={() => setTrustPrompt(null)}
            >
              取消
            </Button>
            <Button disabled={trustingConnection} onClick={() => void trustConnection()}>
              {trustingConnection ? <Spinner data-icon="inline-start" /> : null}
              指纹一致，继续
            </Button>
          </DialogFooter>
        </DialogPanel>
      </Dialog>

      <Dialog open={Boolean(issuesFor)} onOpenChange={(open) => !open && setIssuesFor(null)}>
        <DialogPanel showOverlay={false} className="plugin-repository-dialog sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>{issuesFor?.name || "仓库"}的隔离问题</DialogTitle>
            <DialogDescription>
              有问题的插件不会影响同一仓库中其他插件的同步和安装。
            </DialogDescription>
          </DialogHeader>
          <DialogContent>
            {issuesLoading ? (
              <div className="plugin-repository-loading min-h-24">
                <Spinner />
                <span>正在读取问题</span>
              </div>
            ) : issues.length === 0 ? (
              <p className="py-6 text-center text-[13px] text-muted-foreground">暂无问题</p>
            ) : (
              <ul className="plugin-repository-issue-list">
                {issues.map((issue, index) => (
                  <li key={`${issue.pluginId || issue.pluginRoot || "issue"}:${index}`}>
                    <AlertTriangle className="size-4 shrink-0 text-destructive" />
                    <div className="min-w-0">
                      <p>{issue.pluginId || issue.pluginRoot || "索引项"}</p>
                      <span>{issue.error}</span>
                    </div>
                  </li>
                ))}
              </ul>
            )}
          </DialogContent>
          <DialogFooter>
            <Button variant="outline" onClick={() => setIssuesFor(null)}>
              关闭
            </Button>
          </DialogFooter>
        </DialogPanel>
      </Dialog>
    </div>
  );
}
