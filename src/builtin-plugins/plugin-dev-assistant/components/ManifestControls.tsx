import { useEffect, useRef, useState, type ReactNode } from "react";
import { Plus, Settings2, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogPanel,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
  FieldLegend,
  FieldSet,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";

export function ManifestDetailsDialog({
  title,
  description,
  panelClassName,
  contentClassName,
  children,
}: {
  title: string;
  description: string;
  panelClassName?: string;
  contentClassName?: string;
  children: ReactNode;
}) {
  return (
    <Dialog>
      <DialogTrigger asChild>
        <Button type="button" size="lg" variant="outline" title={`配置 ${title}`}>
          <Settings2 data-icon="inline-start" />
          配置
        </Button>
      </DialogTrigger>
      <DialogPanel
        className={cn(
          "max-h-[min(760px,calc(100vh-2rem))] sm:max-w-2xl",
          panelClassName,
        )}
      >
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        <DialogContent className={cn("flex flex-col gap-6", contentClassName)}>
          {children}
        </DialogContent>
        <DialogFooter>
          <DialogClose asChild>
            <Button type="button" size="lg">完成</Button>
          </DialogClose>
        </DialogFooter>
      </DialogPanel>
    </Dialog>
  );
}

export function StringListField({
  label,
  description,
  items,
  itemLabel,
  placeholder,
  onChange,
}: {
  label: string;
  description?: string;
  items: string[];
  itemLabel?: string;
  placeholder?: string;
  onChange: (items: string[]) => void;
}) {
  return (
    <Field>
      <div className="plugin-dev-field-heading">
        <div>
          <FieldLabel>{label}</FieldLabel>
          {description ? <FieldDescription>{description}</FieldDescription> : null}
        </div>
        <Button
          type="button"
          size="lg"
          variant="outline"
          onClick={() => onChange([...items, ""])}
        >
          <Plus data-icon="inline-start" />
          添加
        </Button>
      </div>
      {items.length > 0 ? (
        <div className="plugin-dev-string-list">
          {items.map((item, index) => (
            <div className="plugin-dev-string-list__row" key={index}>
              <Input
                aria-label={`${itemLabel ?? label} ${index + 1}`}
                placeholder={placeholder}
                value={item}
                onChange={(event) => {
                  const next = [...items];
                  next[index] = event.target.value;
                  onChange(next);
                }}
              />
              <Button
                type="button"
                size="icon-lg"
                variant="ghost"
                aria-label={`删除${itemLabel ?? label}`}
                onClick={() => onChange(items.filter((_, itemIndex) => itemIndex !== index))}
              >
                <Trash2 />
              </Button>
            </div>
          ))}
        </div>
      ) : null}
    </Field>
  );
}

export interface ToggleOption<T extends string> {
  value: T;
  label: string;
  description?: string;
  disabled?: boolean;
  disabledHint?: string;
}

export function ToggleListField<T extends string>({
  legend,
  description,
  options,
  values,
  onChange,
  requireOne = false,
}: {
  legend?: string;
  description?: string;
  options: readonly ToggleOption<T>[];
  values: readonly T[];
  onChange: (values: T[]) => void;
  requireOne?: boolean;
}) {
  return (
    <FieldSet>
      {legend ? <FieldLegend variant="label">{legend}</FieldLegend> : null}
      {description ? <FieldDescription>{description}</FieldDescription> : null}
      <div className="plugin-dev-toggle-grid">
        {options.map((option) => {
          const checked = values.includes(option.value);
          const disabled =
            Boolean(option.disabled) ||
            (requireOne && checked && values.length === 1);
          return (
            <Field
              key={option.value}
              orientation="horizontal"
              className="items-center"
              data-disabled={disabled || undefined}
            >
              <div className="min-w-0 flex-1">
                <FieldLabel className="font-normal">{option.label}</FieldLabel>
                {option.disabled && option.disabledHint ? (
                  <FieldDescription>{option.disabledHint}</FieldDescription>
                ) : option.description ? (
                  <FieldDescription>{option.description}</FieldDescription>
                ) : null}
              </div>
              <Switch
                checked={checked}
                disabled={disabled}
                aria-label={option.label}
                title={option.disabled ? option.disabledHint : undefined}
                onCheckedChange={(nextChecked) => {
                  if (option.disabled) return;
                  onChange(
                    nextChecked
                      ? [...values, option.value]
                      : values.filter((value) => value !== option.value),
                  );
                }}
              />
            </Field>
          );
        })}
      </div>
    </FieldSet>
  );
}

function serializeObject(value: Record<string, unknown>) {
  return JSON.stringify(value, null, 2);
}

const SCHEMA_PROPERTY_TYPE_ITEMS = [
  { value: "string", label: "string" },
  { value: "number", label: "number" },
  { value: "integer", label: "integer" },
  { value: "boolean", label: "boolean" },
  { value: "array", label: "array" },
  { value: "object", label: "object" },
] as const;

type SchemaPropertyType = (typeof SCHEMA_PROPERTY_TYPE_ITEMS)[number]["value"];

type SchemaPropertyRow = {
  name: string;
  type: SchemaPropertyType;
  description: string;
  required: boolean;
};

function readSchemaProperties(schema: Record<string, unknown>): SchemaPropertyRow[] {
  const properties =
    schema.properties &&
    typeof schema.properties === "object" &&
    !Array.isArray(schema.properties)
      ? (schema.properties as Record<string, unknown>)
      : {};
  const required = Array.isArray(schema.required)
    ? schema.required.filter((item): item is string => typeof item === "string")
    : [];
  return Object.entries(properties).map(([name, raw]) => {
    const property =
      raw && typeof raw === "object" && !Array.isArray(raw)
        ? (raw as Record<string, unknown>)
        : {};
    const type =
      typeof property.type === "string" &&
      SCHEMA_PROPERTY_TYPE_ITEMS.some((item) => item.value === property.type)
        ? (property.type as SchemaPropertyType)
        : "string";
    return {
      name,
      type,
      description:
        typeof property.description === "string" ? property.description : "",
      required: required.includes(name),
    };
  });
}

function buildObjectSchema(
  rows: SchemaPropertyRow[],
  additionalProperties: boolean | undefined,
): Record<string, unknown> {
  const properties: Record<string, unknown> = {};
  const required: string[] = [];
  rows.forEach((row, index) => {
    const name = row.name.trim() || `__draft_${index}`;
    const property: Record<string, unknown> = { type: row.type };
    if (row.description.trim()) property.description = row.description.trim();
    properties[name] = property;
    if (row.required && row.name.trim()) required.push(name);
  });
  const schema: Record<string, unknown> = {
    type: "object",
    properties,
  };
  if (required.length > 0) schema.required = required;
  if (additionalProperties === false) schema.additionalProperties = false;
  else if (additionalProperties === true) schema.additionalProperties = true;
  return schema;
}

function displayPropertyName(name: string) {
  return /^__draft_\d+$/.test(name) ? "" : name;
}

export function SchemaObjectField({
  label,
  description,
  value,
  onChange,
}: {
  label: string;
  description?: string;
  value: Record<string, unknown>;
  onChange: (value: Record<string, unknown>) => void;
}) {
  const serialized = serializeObject(value);
  const [rows, setRows] = useState(() =>
    readSchemaProperties(value).map((row) => ({
      ...row,
      name: displayPropertyName(row.name),
    })),
  );
  const [additionalProperties, setAdditionalProperties] = useState<
    boolean | undefined
  >(
    typeof value.additionalProperties === "boolean"
      ? value.additionalProperties
      : undefined,
  );
  const lastCommitted = useRef(serialized);

  useEffect(() => {
    if (serialized === lastCommitted.current) return;
    lastCommitted.current = serialized;
    setRows(
      readSchemaProperties(value).map((row) => ({
        ...row,
        name: displayPropertyName(row.name),
      })),
    );
    setAdditionalProperties(
      typeof value.additionalProperties === "boolean"
        ? value.additionalProperties
        : undefined,
    );
  }, [serialized, value]);

  const commit = (
    nextRows: SchemaPropertyRow[],
    nextAdditional: boolean | undefined = additionalProperties,
  ) => {
    setRows(nextRows);
    setAdditionalProperties(nextAdditional);
    const schema = buildObjectSchema(nextRows, nextAdditional);
    lastCommitted.current = serializeObject(schema);
    onChange(schema);
  };

  return (
    <Field>
      <div className="plugin-dev-field-heading">
        <div>
          <FieldLabel>{label}</FieldLabel>
          {description ? (
            <FieldDescription>{description}</FieldDescription>
          ) : null}
        </div>
        <Button
          type="button"
          size="lg"
          variant="outline"
          onClick={() =>
            commit([
              ...rows,
              {
                name: `field${rows.length + 1}`,
                type: "string",
                description: "",
                required: false,
              },
            ])
          }
        >
          <Plus data-icon="inline-start" />
          添加字段
        </Button>
      </div>
      {rows.length > 0 ? (
        <div className="plugin-dev-schema-list">
          {rows.map((row, index) => (
            <div className="plugin-dev-schema-list__row" key={index}>
              <Input
                className="plugin-dev-schema-list__name"
                aria-label={`字段名 ${index + 1}`}
                placeholder="字段名"
                value={row.name}
                onChange={(event) => {
                  const next = [...rows];
                  next[index] = { ...row, name: event.target.value };
                  commit(next);
                }}
              />
              <div className="plugin-dev-schema-list__type">
                <Select
                  items={[...SCHEMA_PROPERTY_TYPE_ITEMS]}
                  value={row.type}
                  onValueChange={(nextType) => {
                    if (!nextType) return;
                    const next = [...rows];
                    next[index] = {
                      ...row,
                      type: nextType as SchemaPropertyType,
                    };
                    commit(next);
                  }}
                >
                  <SelectTrigger className="w-full" aria-label={`类型 ${index + 1}`}>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      {SCHEMA_PROPERTY_TYPE_ITEMS.map((item) => (
                        <SelectItem key={item.value} value={item.value}>
                          {item.label}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </div>
              <Input
                className="plugin-dev-schema-list__description"
                aria-label={`描述 ${index + 1}`}
                placeholder="描述（可选）"
                value={row.description}
                onChange={(event) => {
                  const next = [...rows];
                  next[index] = { ...row, description: event.target.value };
                  commit(next);
                }}
              />
              <div className="plugin-dev-schema-list__required">
                <Switch
                  size="sm"
                  checked={row.required}
                  aria-label={`必填 ${index + 1}`}
                  onCheckedChange={(checked) => {
                    const next = [...rows];
                    next[index] = { ...row, required: checked };
                    commit(next);
                  }}
                />
                <span>必填</span>
              </div>
              <Button
                type="button"
                size="icon-lg"
                variant="ghost"
                className="plugin-dev-schema-list__remove"
                aria-label={`删除字段 ${index + 1}`}
                onClick={() => commit(rows.filter((_, i) => i !== index))}
              >
                <Trash2 />
              </Button>
            </div>
          ))}
        </div>
      ) : (
        <FieldDescription>暂无字段，点击「添加字段」开始配置。</FieldDescription>
      )}
      <Field orientation="horizontal" className="items-center">
        <FieldLabel className="flex-1 font-normal">
          禁止额外属性（additionalProperties: false）
        </FieldLabel>
        <Switch
          checked={additionalProperties === false}
          onCheckedChange={(checked) =>
            commit(rows, checked ? false : undefined)
          }
        />
      </Field>
    </Field>
  );
}

export function JsonObjectField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: Record<string, unknown>;
  onChange: (value: Record<string, unknown>) => void;
}) {
  const serialized = serializeObject(value);
  const [draft, setDraft] = useState(serialized);
  const [error, setError] = useState<string | null>(null);
  const lastCommitted = useRef(serialized);

  useEffect(() => {
    if (serialized === lastCommitted.current) return;
    lastCommitted.current = serialized;
    setDraft(serialized);
    setError(null);
  }, [serialized]);

  return (
    <Field data-invalid={Boolean(error)}>
      <FieldLabel>{label}</FieldLabel>
      <Textarea
        className="plugin-dev-schema-editor"
        value={draft}
        spellCheck={false}
        aria-invalid={Boolean(error)}
        onChange={(event) => {
          const raw = event.target.value;
          setDraft(raw);
          try {
            const parsed = JSON.parse(raw) as unknown;
            if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
              throw new Error("必须是 JSON 对象");
            }
            const next = parsed as Record<string, unknown>;
            lastCommitted.current = serializeObject(next);
            setError(null);
            onChange(next);
          } catch (cause) {
            setError(cause instanceof Error ? cause.message : String(cause));
          }
        }}
      />
      <FieldError>{error}</FieldError>
    </Field>
  );
}
