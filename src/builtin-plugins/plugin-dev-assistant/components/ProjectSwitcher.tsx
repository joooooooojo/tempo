import { useState, type CSSProperties } from "react";
import { Check, ChevronDown, FolderOpen, Plus } from "lucide-react";
import { Button } from "@/components/ui/button.tsx";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover.tsx";
import { cn } from "@/lib/utils.ts";
import {
  projectFolderName,
  projectMarkColor,
  projectMonogram,
} from "@/builtin-plugins/plugin-dev-assistant/pages/shared.tsx";
import type { PluginDevProject } from "@/types";

type ProjectSwitcherProps = {
  projects: PluginDevProject[];
  activeProjectId: string | null;
  disabled?: boolean;
  onSelect: (projectId: string) => void;
  onCreate: () => void;
  onOpen: () => void;
};

export function ProjectSwitcher({
  projects,
  activeProjectId,
  disabled = false,
  onSelect,
  onCreate,
  onOpen,
}: ProjectSwitcherProps) {
  const [open, setOpen] = useState(false);
  const active =
    projects.find((project) => project.id === activeProjectId) ?? null;
  const label = active
    ? active.name ||
      active.pluginId ||
      projectFolderName(active.rootPath) ||
      "未命名项目"
    : "选择项目";
  const markPath = active?.rootPath ?? "";

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="outline"
          size="lg"
          disabled={disabled}
          className="plugin-dev-switcher h-[38px] max-w-[min(360px,52vw)] gap-2 px-2"
          data-no-drag
          aria-label="切换插件项目"
          aria-expanded={open}
        >
          <span
            className="plugin-dev-project__mark"
            aria-hidden="true"
            style={
              markPath
                ? ({
                    "--plugin-dev-project-mark": projectMarkColor(markPath),
                  } as CSSProperties)
                : ({
                    "--plugin-dev-project-mark": "hsl(var(--muted-foreground))",
                  } as CSSProperties)
            }
          >
            {markPath ? projectMonogram(markPath) : "?"}
          </span>
          <span className="plugin-dev-switcher__label">{label}</span>
          <ChevronDown data-icon="inline-end" aria-hidden="true" />
        </Button>
      </PopoverTrigger>
      <PopoverContent
        align="start"
        sideOffset={6}
        className="plugin-dev-switcher-menu w-[min(320px,calc(100vw-32px))] gap-0 p-1.5"
      >
        <div className="flex flex-col gap-0.5">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="w-full justify-start border-0"
            onClick={() => {
              setOpen(false);
              onCreate();
            }}
          >
            <Plus data-icon="inline-start" />
            新建项目
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="w-full justify-start border-0"
            onClick={() => {
              setOpen(false);
              onOpen();
            }}
          >
            <FolderOpen data-icon="inline-start" />
            打开项目
          </Button>
        </div>
        <div className="plugin-dev-switcher-menu__divider" />
        <div className="plugin-dev-switcher-menu__section">
          <div className="plugin-dev-switcher-menu__heading">打开项目</div>
          {projects.length === 0 ? (
            <p className="plugin-dev-switcher-menu__empty">
              还没有项目，先新建或打开一个目录
            </p>
          ) : (
            <div className="plugin-dev-switcher-menu__list">
              {projects.map((project) => {
                const selected = project.id === activeProjectId;
                const name =
                  project.name ||
                  project.pluginId ||
                  projectFolderName(project.rootPath) ||
                  "未命名项目";
                return (
                  <Button
                    key={project.id}
                    type="button"
                    variant={selected ? "secondary" : "ghost"}
                    size="sm"
                    className={cn(
                      "h-auto w-full justify-start gap-2.5 py-2 whitespace-normal",
                    )}
                    onClick={() => {
                      setOpen(false);
                      if (!selected) onSelect(project.id);
                    }}
                  >
                    <span
                      className="plugin-dev-project__mark"
                      aria-hidden="true"
                      style={
                        {
                          "--plugin-dev-project-mark": projectMarkColor(
                            project.rootPath,
                          ),
                        } as CSSProperties
                      }
                    >
                      {projectMonogram(project.rootPath)}
                    </span>
                    <span className="plugin-dev-switcher-menu__meta">
                      <strong>{name}</strong>
                      <small title={project.rootPath}>{project.rootPath}</small>
                    </span>
                    {selected ? (
                      <Check
                        className="plugin-dev-switcher-menu__check"
                        aria-hidden="true"
                      />
                    ) : (
                      <span className="plugin-dev-switcher-menu__check" />
                    )}
                  </Button>
                );
              })}
            </div>
          )}
        </div>
      </PopoverContent>
    </Popover>
  );
}
