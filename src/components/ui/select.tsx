import * as React from "react";
import * as SelectPrimitive from "@radix-ui/react-select";
import { Check, ChevronDown, Search } from "lucide-react";
import { cn } from "@/lib/utils";

const Select = SelectPrimitive.Root;
const SelectGroup = SelectPrimitive.Group;
const SelectValue = SelectPrimitive.Value;

type SelectContentProps = React.ComponentPropsWithoutRef<typeof SelectPrimitive.Content> & {
  searchable?: boolean;
  searchPlaceholder?: string;
  emptyText?: string;
  viewportClassName?: string;
};

type SelectChildProps = {
  children?: React.ReactNode;
  textValue?: string;
  value?: string;
};

const SelectTrigger = React.forwardRef<
  React.ComponentRef<typeof SelectPrimitive.Trigger>,
  React.ComponentPropsWithoutRef<typeof SelectPrimitive.Trigger>
>(({ className, children, ...props }, ref) => (
  <SelectPrimitive.Trigger
    ref={ref}
    className={cn(
      "flex h-9 w-full min-w-0 items-center justify-between overflow-hidden rounded-md border border-input bg-transparent px-3 py-2 text-sm shadow-sm ring-offset-background placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring disabled:cursor-not-allowed disabled:opacity-50",
      className
    )}
    {...props}
  >
    <span className="min-w-0 flex-1 truncate text-left">{children}</span>
    <SelectPrimitive.Icon asChild>
      <ChevronDown className="ml-2 h-4 w-4 shrink-0 opacity-50" />
    </SelectPrimitive.Icon>
  </SelectPrimitive.Trigger>
));
SelectTrigger.displayName = SelectPrimitive.Trigger.displayName;

const SelectContent = React.forwardRef<
  React.ComponentRef<typeof SelectPrimitive.Content>,
  SelectContentProps
>(({
  className,
  children,
  position = "popper",
  searchable = false,
  searchPlaceholder = "搜索",
  emptyText = "没有匹配结果",
  viewportClassName,
  sideOffset = 6,
  ...props
}, ref) => {
  const [query, setQuery] = React.useState("");
  const searchInputRef = React.useRef<HTMLInputElement>(null);
  const normalizedQuery = normalizeSearchText(query);
  const filteredChildren = normalizedQuery
    ? filterSelectChildren(children, normalizedQuery)
    : children;
  const hasResults = React.Children.count(filteredChildren) > 0;

  React.useEffect(() => {
    if (!searchable) return;

    const id = window.setTimeout(() => searchInputRef.current?.focus(), 0);
    return () => window.clearTimeout(id);
  }, [searchable]);

  return (
    <SelectPrimitive.Portal>
      <SelectPrimitive.Content
        ref={ref}
        className={cn(
          "select-content relative z-50 max-h-[min(18rem,var(--radix-select-content-available-height))] min-w-[8rem] overflow-hidden rounded-lg border border-border/70 bg-popover/95 text-popover-foreground shadow-xl shadow-black/10 backdrop-blur-xl",
          position === "popper" && "w-[var(--radix-select-trigger-width)] min-w-[var(--radix-select-trigger-width)]",
          className
        )}
        position={position}
        sideOffset={sideOffset}
        {...props}
      >
        {searchable && (
          <div className="border-b border-border/60 px-2 pb-2 pt-3">
            <div className="flex h-8 items-center gap-2 rounded-md bg-foreground/[0.045] px-2 text-muted-foreground ring-1 ring-transparent transition focus-within:bg-background/80 focus-within:ring-ring/40">
              <Search className="h-3.5 w-3.5 shrink-0" />
              <input
                ref={searchInputRef}
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key !== "Escape") event.stopPropagation();
                }}
                placeholder={searchPlaceholder}
                className="h-full min-w-0 flex-1 bg-transparent text-[13px] text-foreground outline-none placeholder:text-muted-foreground"
              />
            </div>
          </div>
        )}
        <SelectPrimitive.Viewport
          className={cn(
            "select-viewport max-h-[13.5rem] overflow-y-auto p-1",
            searchable && "max-h-[11rem]",
            viewportClassName
          )}
        >
          {hasResults ? (
            filteredChildren
          ) : (
            <div className="px-3 py-6 text-center text-[13px] text-muted-foreground">
              {emptyText}
            </div>
          )}
        </SelectPrimitive.Viewport>
      </SelectPrimitive.Content>
    </SelectPrimitive.Portal>
  );
});
SelectContent.displayName = SelectPrimitive.Content.displayName;

const SelectItem = React.forwardRef<
  React.ComponentRef<typeof SelectPrimitive.Item>,
  React.ComponentPropsWithoutRef<typeof SelectPrimitive.Item>
>(({ className, children, ...props }, ref) => (
  <SelectPrimitive.Item
    ref={ref}
    className={cn(
      "grid w-full min-w-0 cursor-pointer select-none grid-cols-[minmax(0,1fr)_1.25rem] items-center gap-2 rounded-md py-1.5 pl-2.5 pr-2 text-sm outline-none transition-colors focus:bg-accent focus:text-accent-foreground data-[disabled]:pointer-events-none data-[disabled]:cursor-default data-[disabled]:opacity-50",
      className
    )}
    {...props}
  >
    <span className="block min-w-0 overflow-hidden text-ellipsis whitespace-nowrap">
      <SelectPrimitive.ItemText>{children}</SelectPrimitive.ItemText>
    </span>
    <span className="flex h-4 w-4 shrink-0 items-center justify-center">
      <SelectPrimitive.ItemIndicator>
        <Check className="h-4 w-4" />
      </SelectPrimitive.ItemIndicator>
    </span>
  </SelectPrimitive.Item>
));
SelectItem.displayName = SelectPrimitive.Item.displayName;

function normalizeSearchText(value: string) {
  return value.trim().toLocaleLowerCase();
}

function getNodeText(node: React.ReactNode): string {
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(getNodeText).join(" ");
  if (React.isValidElement(node)) {
    const props = node.props as SelectChildProps;
    return props.textValue ?? getNodeText(props.children);
  }
  return "";
}

function filterSelectChildren(
  children: React.ReactNode,
  normalizedQuery: string
): React.ReactNode[] {
  return React.Children.toArray(children).flatMap((child) => {
    if (!React.isValidElement(child)) {
      return normalizeSearchText(getNodeText(child)).includes(normalizedQuery) ? [child] : [];
    }

    const childProps = child.props as SelectChildProps;
    const childText = normalizeSearchText(childProps.textValue ?? getNodeText(childProps.children));

    if (childProps.value !== undefined) {
      return childText.includes(normalizedQuery) ? [child] : [];
    }

    const filteredChildren = filterSelectChildren(childProps.children, normalizedQuery);
    if (filteredChildren.length > 0) {
      return [
        React.cloneElement(
          child as React.ReactElement<SelectChildProps>,
          undefined,
          filteredChildren
        ),
      ];
    }

    return childText.includes(normalizedQuery) ? [child] : [];
  });
}

export { Select, SelectGroup, SelectValue, SelectTrigger, SelectContent, SelectItem };
