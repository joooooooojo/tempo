import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogPanel,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { cn } from "@/lib/utils";
import type { HostsBackup, HostsProfileKind } from "@/types";

export const REFRESH_INTERVAL_ITEMS = [
  { value: "0", label: "仅手动" },
  { value: "3600", label: "1 小时" },
  { value: "21600", label: "6 小时" },
  { value: "86400", label: "1 天" },
] as const;

export type CreateLocalMode = "blank" | "file";

export type CreateHostsDraft = {
  kind: HostsProfileKind;
  name: string;
  localMode: CreateLocalMode;
  importPath: string;
  remoteUrl: string;
  refreshIntervalSecs: number;
};

export const EMPTY_CREATE_DRAFT: CreateHostsDraft = {
  kind: "local",
  name: "",
  localMode: "blank",
  importPath: "",
  remoteUrl: "",
  refreshIntervalSecs: 3600,
};

export function HostsDialogs({
  formOpen,
  onFormOpenChange,
  formMode,
  draft,
  onDraftChange,
  onPickImportFile,
  onSubmit,
  saving,
  backupOpen,
  onBackupOpenChange,
  backups,
  onRestore,
}: {
  formOpen: boolean;
  onFormOpenChange: (open: boolean) => void;
  formMode: "create" | "edit";
  draft: CreateHostsDraft;
  onDraftChange: (next: CreateHostsDraft) => void;
  onPickImportFile: () => void;
  onSubmit: () => void;
  saving: boolean;
  backupOpen: boolean;
  onBackupOpenChange: (open: boolean) => void;
  backups: HostsBackup[];
  onRestore: (backup: HostsBackup) => void;
}) {
  const editing = formMode === "edit";
  const canSubmit =
    draft.name.trim().length > 0 &&
    (draft.kind === "local"
      ? editing || draft.localMode === "blank" || draft.importPath.trim().length > 0
      : draft.remoteUrl.trim().length > 0);

  return (
    <>
      <Dialog open={formOpen} onOpenChange={onFormOpenChange}>
        <DialogPanel className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{editing ? "编辑 hosts" : "添加 hosts"}</DialogTitle>
          </DialogHeader>
          <DialogContent className="space-y-4">
            <div className="space-y-2">
              <Label>Hosts 类型</Label>
              <div className="grid grid-cols-2 gap-2">
                {(
                  [
                    ["local", "本地"],
                    ["remote", "远程"],
                  ] as const
                ).map(([value, label]) => {
                  const selected = draft.kind === value;
                  return (
                    <button
                      key={value}
                      type="button"
                      disabled={editing}
                      className={cn(
                        "rounded-lg border px-3 py-2 text-[13px] transition-colors",
                        selected
                          ? "border-primary/50 bg-primary/10 font-medium"
                          : "border-border/60 hover:bg-foreground/5",
                        editing && "cursor-not-allowed opacity-60",
                      )}
                      onClick={() => {
                        if (editing) return;
                        onDraftChange({ ...draft, kind: value });
                      }}
                    >
                      {label}
                    </button>
                  );
                })}
              </div>
              {editing ? (
                <p className="text-[11px] text-muted-foreground">编辑时不可切换本地 / 远程</p>
              ) : null}
            </div>

            <div className="space-y-2">
              <Label htmlFor="hosts-form-name">标题</Label>
              <Input
                id="hosts-form-name"
                value={draft.name}
                onChange={(e) => onDraftChange({ ...draft, name: e.target.value })}
                placeholder="请输入标题"
                autoFocus
              />
            </div>

            {draft.kind === "local" ? (
              editing ? null : (
                <div className="space-y-2">
                  <Label>本地来源</Label>
                  <div className="grid grid-cols-2 gap-2">
                    {(
                      [
                        ["blank", "空白编辑"],
                        ["file", "选择文件"],
                      ] as const
                    ).map(([value, label]) => (
                      <button
                        key={value}
                        type="button"
                        className={cn(
                          "rounded-lg border px-3 py-2 text-[13px] transition-colors",
                          draft.localMode === value
                            ? "border-primary/50 bg-primary/10 font-medium"
                            : "border-border/60 hover:bg-foreground/5",
                        )}
                        onClick={() => onDraftChange({ ...draft, localMode: value })}
                      >
                        {label}
                      </button>
                    ))}
                  </div>
                  {draft.localMode === "file" ? (
                    <div className="flex gap-2">
                      <Input
                        value={draft.importPath}
                        readOnly
                        placeholder="选择本地 hosts 文件"
                        className="flex-1"
                      />
                      <Button type="button" variant="outline" onClick={onPickImportFile}>
                        浏览
                      </Button>
                    </div>
                  ) : null}
                </div>
              )
            ) : (
              <>
                <div className="space-y-2">
                  <Label htmlFor="hosts-form-url">URL</Label>
                  <Input
                    id="hosts-form-url"
                    value={draft.remoteUrl}
                    onChange={(e) =>
                      onDraftChange({ ...draft, remoteUrl: e.target.value })
                    }
                    placeholder="https://example.com/hosts"
                    spellCheck={false}
                  />
                </div>
                <div className="space-y-2">
                  <Label>自动刷新</Label>
                  <Select
                    items={[...REFRESH_INTERVAL_ITEMS]}
                    value={String(draft.refreshIntervalSecs)}
                    onValueChange={(value) =>
                      value &&
                      onDraftChange({
                        ...draft,
                        refreshIntervalSecs: Number(value),
                      })
                    }
                  >
                    <SelectTrigger className="w-full">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectGroup>
                        {REFRESH_INTERVAL_ITEMS.map((item) => (
                          <SelectItem key={item.value} value={item.value}>
                            {item.label}
                          </SelectItem>
                        ))}
                      </SelectGroup>
                    </SelectContent>
                  </Select>
                </div>
              </>
            )}
          </DialogContent>
          <DialogFooter>
            <Button variant="outline" onClick={() => onFormOpenChange(false)}>
              取消
            </Button>
            <Button onClick={() => onSubmit()} disabled={saving || !canSubmit}>
              确定
            </Button>
          </DialogFooter>
        </DialogPanel>
      </Dialog>

      <Dialog open={backupOpen} onOpenChange={onBackupOpenChange}>
        <DialogPanel className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>备份记录</DialogTitle>
          </DialogHeader>
          <DialogContent className="space-y-1.5 !px-4 !py-3">
            {backups.length === 0 ? (
              <p className="py-6 text-center text-[12px] text-muted-foreground">暂无备份</p>
            ) : (
              backups.map((backup) => (
                <button
                  key={backup.id}
                  type="button"
                  className="w-full rounded-lg border border-border/50 px-3 py-2.5 text-left transition-colors hover:bg-foreground/5"
                  onClick={() => onRestore(backup)}
                  title={backup.preview}
                >
                  <div className="text-[13px] font-medium">{backup.createdAt}</div>
                  <div className="mt-0.5 truncate text-[11px] text-muted-foreground">
                    {backup.source}
                    {backup.preview ? ` · ${backup.preview}` : ""}
                  </div>
                </button>
              ))
            )}
          </DialogContent>
          <DialogFooter>
            <Button variant="outline" onClick={() => onBackupOpenChange(false)}>
              关闭
            </Button>
          </DialogFooter>
        </DialogPanel>
      </Dialog>
    </>
  );
}
