import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogPanel,
  DialogTitle,
} from "@/components/ui/dialog";
import type { HostsBackup } from "@/types";

export function HostsDialogs({
  createOpen,
  onCreateOpenChange,
  profileName,
  onProfileNameChange,
  onCreate,
  saving,
  backupOpen,
  onBackupOpenChange,
  backups,
  onRestore,
}: {
  createOpen: boolean;
  onCreateOpenChange: (open: boolean) => void;
  profileName: string;
  onProfileNameChange: (value: string) => void;
  onCreate: () => void;
  saving: boolean;
  backupOpen: boolean;
  onBackupOpenChange: (open: boolean) => void;
  backups: HostsBackup[];
  onRestore: (backup: HostsBackup) => void;
}) {
  return (
    <>
      <Dialog open={createOpen} onOpenChange={onCreateOpenChange}>
        <DialogPanel className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>新建自定义配置</DialogTitle>
          </DialogHeader>
          <DialogContent>
            <Input
              value={profileName}
              onChange={(e) => onProfileNameChange(e.target.value)}
              placeholder="例如：公司环境 / 测试环境"
              autoFocus
            />
          </DialogContent>
          <DialogFooter>
            <Button variant="outline" onClick={() => onCreateOpenChange(false)}>
              取消
            </Button>
            <Button onClick={() => onCreate()} disabled={saving}>创建</Button>
          </DialogFooter>
        </DialogPanel>
      </Dialog>

      <Dialog open={backupOpen} onOpenChange={onBackupOpenChange}>
        <DialogPanel className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>备份记录</DialogTitle>
          </DialogHeader>
          <DialogContent className="max-h-[min(420px,55vh)] space-y-1.5 overflow-y-auto px-4 py-3">
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
