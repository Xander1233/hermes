import { useEffect, useMemo, useState } from "react";
import { useFetcher } from "react-router";
import { RiInformationLine, RiLoader4Line } from "@remixicon/react";
import { toast } from "sonner";

import {
  type CreateRecordRequest,
  type DnsRecord,
  type RecordType,
  type ZoneDetail,
} from "~/lib/api";
import { firstFieldErrors } from "~/lib/mutation";
import {
  CAA_TAGS,
  RECORD_TYPE_INFO,
  SELECTABLE_RECORD_TYPES,
  TTL_PRESETS,
  toFqdn,
  toRelativeName,
  validateRecord,
  type ExtraField,
} from "~/lib/dns";

import { Button } from "~/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "~/components/ui/dialog";
import { Input } from "~/components/ui/input";
import { Label } from "~/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "~/components/ui/select";
import { Textarea } from "~/components/ui/textarea";

import type { clientAction } from "~/routes/zone.records";

interface FormState {
  name: string;
  type: RecordType;
  ttl: string;
  data: string;
  preference: string;
  priority: string;
  weight: string;
  port: string;
  flags: string;
  tag: string;
  notes: string;
}

const EXTRA_FIELD_LABEL: Record<ExtraField, string> = {
  preference: "Priority",
  priority: "Priority",
  weight: "Weight",
  port: "Port",
  flags: "Flags",
  tag: "Tag",
};

function emptyForm(): FormState {
  return {
    name: "",
    type: "A",
    ttl: "300",
    data: "",
    preference: "10",
    priority: "0",
    weight: "0",
    port: "443",
    flags: "0",
    tag: "issue",
    notes: "",
  };
}

function fromRecord(record: DnsRecord, apex: string): FormState {
  return {
    name: toRelativeName(record.name, apex),
    // apex CNAMEs are stored as ALIASes, but the user should see it as a CNAME.
    type: record.type === "ALIAS" ? "CNAME" : record.type,
    ttl: String(record.ttl),
    data: record.data ?? "",
    preference: String(record.preference ?? 10),
    priority: String(record.priority ?? 0),
    weight: String(record.weight ?? 0),
    port: String(record.port ?? 443),
    flags: String(record.flags ?? 0),
    tag: record.tag ?? "issue",
    notes: record.notes ?? "",
  };
}

function toRequest(form: FormState, apex: string): CreateRecordRequest {
  const info = RECORD_TYPE_INFO[form.type];
  const numeric = (field: ExtraField) => info.extraFields.includes(field) ? Number(form[field]) : null;

  return {
    name: toFqdn(form.name, apex),
    type: form.type,
    ttl: Number(form.ttl),
    data: form.data.trim(),
    preference: numeric("preference"),
    priority: numeric("priority"),
    weight: numeric("weight"),
    port: numeric("port"),
    flags: numeric("flags"),
    tag: info.extraFields.includes("tag") ? form.tag : null,
    notes: form.notes.trim() === "" ? null : form.notes.trim(),
  };
}

export function RecordDialog({
  zone,
  record,
  open,
  onOpenChange,
}: {
  zone: ZoneDetail
  record?: DnsRecord
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const fetcher = useFetcher<typeof clientAction>();
  const [form, setForm] = useState<FormState>(emptyForm);
  const [errors, setErrors] = useState<Record<string, string>>({});

  const submitting = fetcher.state !== "idle";

  useEffect(() => {
    if (open) {
      setForm(record ? fromRecord(record, zone.apex) : emptyForm());
      setErrors({});
    }
  }, [ open, record, zone.apex ]);

  useEffect(() => {
    if (submitting || !fetcher.data) return;

    if (!fetcher.data.ok) {
      setErrors(firstFieldErrors(fetcher.data.fieldErrors));
      
      if (Object.keys(fetcher.data.fieldErrors).length === 0) {
        toast.error(fetcher.data.message);
      }
      return;
    }
    
    toast.success(record ? "Record updated." : "Record created.");
    onOpenChange(false);
  }, [ submitting, fetcher.data, record, onOpenChange ]);

  const info = RECORD_TYPE_INFO[form.type];
  const fqdn = useMemo(() => toFqdn(form.name, zone.apex),[ form.name, zone.apex ]);
  const willFlatten = form.type === "CNAME" && fqdn === zone.apex;

  function set<K extends keyof FormState>(key: K, value: FormState[K]) {
    setForm((previous) => ({ ...previous, [key]: value }));
    setErrors(({ [key]: _removed, ...rest }) => rest);
  }

  function submit(event: React.FormEvent) {
    event.preventDefault();
    const body = toRequest(form, zone.apex);

    const effective = willFlatten ? { ...body, type: "ALIAS" as const } : body;
    const localErrors = validateRecord(effective, zone.apex);

    if (Object.keys(localErrors).length > 0) {
      setErrors(localErrors);
      return;
    }

    setErrors({});
    
    fetcher.submit(
      record ? { intent: "update", recordId: record.id, body } : { intent: "create", body },
      {
        method: "post",
        encType: "application/json",
        action: `/zones/${zone.id}?index`,
      }
    );
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <form onSubmit={submit}>
          <DialogHeader>
            <DialogTitle>{record ? "Edit record" : "Add record"}</DialogTitle>
            <DialogDescription>{info.description}</DialogDescription>
          </DialogHeader>

          <div className="my-6 space-y-4">
            <div className="grid gap-4 sm:grid-cols-[8rem_1fr]">
              <FormField label="Type">
                <Select
                  value={form.type}
                  onValueChange={(value) => set("type", value as RecordType)}
                  items={SELECTABLE_RECORD_TYPES.map((type) => ({
                    value: type,
                    label: type,
                  }))}
                >
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {SELECTABLE_RECORD_TYPES.map((type) => (
                      <SelectItem key={type} value={type}>
                        {type}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </FormField>

              <FormField label="Name" error={errors.name} hint={fqdn}>
                <Input
                  className="font-data"
                  value={form.name}
                  onChange={(event) => set("name", event.target.value)}
                  placeholder="@ for the apex, or www"
                  aria-invalid={errors.name ? true : undefined}
                />
              </FormField>
            </div>

            {willFlatten && (
              <div className="flex gap-3 rounded-lg border border-primary/25 bg-primary-subtle/60 px-3.5 py-3">
                <RiInformationLine
                  aria-hidden="true"
                  className="mt-0.5 size-4 shrink-0 text-primary-ink"
                />
                <div className="space-y-1 text-sm">
                  <p className="font-medium">This CNAME will be flattened</p>
                  <p className="text-muted-foreground">
                    A CNAME can&apos;t sit at the apex beside the mandatory SOA and NS records. Hermes stores it as an
                    ALIAS and answers with the target&apos;s current A/AAAA addresses. It still reads as a CNAME here.
                  </p>
                </div>
              </div>
            )}

            <FormField label={info.dataLabel} error={errors.data}>
              {form.type === "TXT" ? (
                <Textarea
                  className="font-data"
                  value={form.data}
                  onChange={(event) => set("data", event.target.value)}
                  placeholder={info.dataPlaceholder}
                  rows={3}
                  aria-invalid={errors.data ? true : undefined}
                />
              ) : (
                <Input
                  className="font-data"
                  value={form.data}
                  onChange={(event) => set("data", event.target.value)}
                  placeholder={info.dataPlaceholder}
                  aria-invalid={errors.data ? true : undefined}
                />
              )}
            </FormField>

            {info.extraFields.length > 0 && (
              <div className="grid gap-4 sm:grid-cols-3">
                {info.extraFields.map((field) => (
                  <FormField
                    key={field}
                    label={EXTRA_FIELD_LABEL[field]}
                    error={errors[field]}
                  >
                    {field === "tag" ? (
                      <Select
                        value={form.tag}
                        onValueChange={(value) => set("tag", String(value))}
                        items={CAA_TAGS.map((tag) => ({
                          value: tag,
                          label: tag,
                        }))}
                      >
                        <SelectTrigger className="w-full">
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          {CAA_TAGS.map((tag) => (
                            <SelectItem key={tag} value={tag}>
                              {tag}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                    ) : (
                      <Input
                        type="number"
                        value={form[field]}
                        onChange={(event) => set(field, event.target.value)}
                        aria-invalid={errors[field] ? true : undefined}
                      />
                    )}
                  </FormField>
                ))}
              </div>
            )}

            <FormField label="TTL" error={errors.ttl}>
              <Select
                value={form.ttl}
                onValueChange={(value) => set("ttl", String(value))}
                items={TTL_PRESETS.map((preset) => ({
                  value: String(preset.value),
                  label: preset.label,
                }))}
              >
                <SelectTrigger className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {TTL_PRESETS.map((preset) => (
                    <SelectItem key={preset.value} value={String(preset.value)}>
                      {preset.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </FormField>

            <FormField
              label="Notes"
              hint="Only visible here. Never published to the DNS servers."
            >
              <Textarea
                value={form.notes}
                onChange={(event) => set("notes", event.target.value)}
                placeholder="Why this record exists, who owns it, when it can be removed…"
                rows={2}
              />
            </FormField>
          </div>

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={submitting}>
              {submitting && <RiLoader4Line className="animate-spin" />}
              {record ? "Save changes" : "Add record"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function FormField({
  label,
  error,
  hint,
  children,
}: {
  label: string
  error?: string
  hint?: string
  children: React.ReactNode
}) {
  return (
    <div className="space-y-2">
      <Label>{label}</Label>
      {children}
      {error ? (
        <p className="text-sm text-destructive">{error}</p>
      ) : hint ? (
        <p className="truncate text-xs text-muted-foreground">{hint}</p>
      ) : null}
    </div>
  );
}
