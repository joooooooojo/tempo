import { useEffect, useState } from "react";
import { Braces, Cable, Code2, FileJson2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { tabsListVariants } from "@/components/ui/tabs";
import { cn } from "@/lib/utils";
import type {
  ManifestMode,
  WorkspaceTab,
} from "@/builtin-plugins/plugin-dev-assistant/pages/shared";

const MANIFEST_MODE_ITEMS = [
  { value: "visual" as const, label: "可视化", icon: Braces },
  { value: "json" as const, label: "JSON", icon: Code2 },
];

type WorkspaceTabsProps = {
  value: WorkspaceTab;
  onChange: (value: WorkspaceTab) => void;
  manifestMode: ManifestMode;
  onManifestModeChange: (mode: ManifestMode) => void;
};

export function WorkspaceTabs({
  value,
  onChange,
  manifestMode,
  onManifestModeChange,
}: WorkspaceTabsProps) {
  const manifestActive = value === "manifest";
  const [manifestMenuOpen, setManifestMenuOpen] = useState(false);
  const activeMode =
    MANIFEST_MODE_ITEMS.find((item) => item.value === manifestMode) ??
    MANIFEST_MODE_ITEMS[0];
  const label = `配置 · ${activeMode.label}`;

  useEffect(() => {
    if (!manifestActive) setManifestMenuOpen(false);
  }, [manifestActive]);

  return (
    <div
      role="tablist"
      aria-label="工作区"
      className={cn(
        tabsListVariants({ variant: "default" }),
        "plugin-dev-workspace-tabs h-[38px]",
      )}
    >
      <Select
        items={MANIFEST_MODE_ITEMS}
        value={manifestMode}
        open={manifestMenuOpen}
        onOpenChange={(next) => {
          if (!manifestActive) {
            onChange("manifest");
            setManifestMenuOpen(false);
            return;
          }
          setManifestMenuOpen(next);
        }}
        onValueChange={(next) => {
          if (!next) return;
          onManifestModeChange(next as ManifestMode);
        }}
      >
        <SelectTrigger
          size="sm"
          aria-selected={manifestActive}
          className={cn(
            "plugin-dev-manifest-select h-[calc(100%-1px)] min-w-0 gap-1.5 border-0 bg-transparent px-2.5 text-[0.8rem] text-foreground/60 shadow-none",
            "hover:bg-background/60 hover:text-foreground",
            "focus-visible:border-0 focus-visible:ring-0",
            "[&_svg]:!size-3.5",
            manifestActive && "bg-background text-foreground",
          )}
          onPointerDown={(event) => {
            if (manifestActive) return;
            event.preventDefault();
            onChange("manifest");
          }}
          onClick={(event) => {
            if (manifestActive) return;
            event.preventDefault();
            onChange("manifest");
          }}
        >
          <FileJson2 className="size-3.5 shrink-0" aria-hidden="true" />
          <SelectValue>{label}</SelectValue>
        </SelectTrigger>
        <SelectContent align="start" className="min-w-[9.5rem]">
          <SelectGroup>
            {MANIFEST_MODE_ITEMS.map((item) => {
              const Icon = item.icon;
              return (
                <SelectItem key={item.value} value={item.value}>
                  <span className="inline-flex min-w-0 items-center gap-2">
                    <Icon className="size-3.5 shrink-0" aria-hidden="true" />
                    <span className="truncate">{item.label}</span>
                  </span>
                </SelectItem>
              );
            })}
          </SelectGroup>
        </SelectContent>
      </Select>

      <Button
        type="button"
        role="tab"
        aria-selected={value === "runtime"}
        variant="ghost"
        size="sm"
        className={cn(
          "h-[calc(100%-1px)] flex-none border-0 px-2.5 text-foreground/60 shadow-none hover:text-foreground active:translate-y-0",
          "[&_svg]:!size-3.5",
          value === "runtime" && "bg-background text-foreground",
        )}
        onClick={() => onChange("runtime")}
      >
        <Cable className="size-3.5 shrink-0" aria-hidden="true" />
        连接
      </Button>
    </div>
  );
}
