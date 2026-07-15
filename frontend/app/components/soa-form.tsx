import { useEffect, useState } from "react";
import { useFetcher } from "react-router";
import { RiLoader4Line } from "@remixicon/react";
import { toast } from "sonner";

import { type Soa, type ZoneDetail } from "~/lib/api";
import { firstFieldErrors } from "~/lib/mutation";
import { formatTtl } from "~/lib/dns";
import { Button } from "~/components/ui/button";
import { Input } from "~/components/ui/input";
import { Label } from "~/components/ui/label";

import type { clientAction } from "~/routes/zone.soa";

const TIMERS = [
  {
    key: "refresh",
    label: "Refresh",
    hint: "How often a secondary re-checks the serial.",
  },
  {
    key: "retry",
    label: "Retry",
    hint: "Wait before retrying a failed refresh.",
  },
  {
    key: "expire",
    label: "Expire",
    hint: "When a secondary gives up and stops answering.",
  },
  {
    key: "minimum",
    label: "Negative TTL",
    hint: "How long resolvers cache an NXDOMAIN for this zone.",
  },
  {
    key: "ttl",
    label: "SOA record TTL",
    hint: "TTL of the SOA record itself.",
  },
] as const satisfies ReadonlyArray<{
  key: keyof Soa
  label: string
  hint: string
}>;

export function SoaForm({ zone }: { zone: ZoneDetail }) {
  const fetcher = useFetcher<typeof clientAction>();
  const [form, setForm] = useState<Soa>(zone.soa);

  useEffect(() => setForm(zone.soa), [ zone.soa ]);

  const saving = fetcher.state !== "idle";
  const result = fetcher.data;
  const errors = result && !result.ok ? firstFieldErrors(result.fieldErrors) : {};
  const dirty = JSON.stringify(form) !== JSON.stringify(zone.soa);

  useEffect(() => {
    if (saving || !result) return;

    if (result.ok) {
      toast.success("Start of authority updated.");
    } else if (Object.keys(result.fieldErrors).length === 0) {
      toast.error(result.message);
    }
  }, [ saving, result ]);

  function set<K extends keyof Soa>(key: K, value: Soa[K]) {
    setForm((previous) => ({ ...previous, [key]: value }))
  }

  return (
    <fetcher.Form method="post" className="max-w-3xl">
      <p className="max-w-[68ch] text-sm text-muted-foreground">
        The zone's metadata record. The defaults are sensible — these only need
        touching if you know why. The serial is managed for you and increments on every change.
      </p>

      <div className="mt-6 grid gap-5 sm:grid-cols-2">
        <Field
          label="Primary nameserver"
          htmlFor="mname"
          error={errors.mname}
          hint="MNAME"
        >
          <Input
            id="mname"
            name="mname"
            className="font-data"
            value={form.mname}
            onChange={(event) => set("mname", event.target.value)}
            aria-invalid={errors.mname ? true : undefined}
          />
        </Field>

        <Field
          label="Responsible party"
          htmlFor="rname"
          error={errors.rname}
          hint="RNAME — an email address written in DNS form."
        >
          <Input
            id="rname"
            name="rname"
            className="font-data"
            value={form.rname}
            onChange={(event) => set("rname", event.target.value)}
            aria-invalid={errors.rname ? true : undefined}
          />
        </Field>
      </div>

      <div className="mt-5 max-w-xs">
        <Field
          label="Serial"
          htmlFor="serial"
          hint="Server-managed. It must never decrease, or secondaries refuse the transfer."
        >
          <Input
            id="serial"
            className="font-data"
            value={form.serial}
            readOnly
            disabled
          />
          <input type="hidden" name="serial" value={form.serial} />
        </Field>
      </div>

      <div className="mt-8 border-t pt-6">
        <h3 className="text-sm font-medium">Timers</h3>
        <div className="mt-4 grid gap-5 sm:grid-cols-2 lg:grid-cols-3">
          {TIMERS.map(({ key, label, hint }) => (
            <Field
              key={key}
              label={label}
              htmlFor={key}
              error={errors[key]}
              hint={`${hint} Now ${formatTtl(Number(form[key]))}.`}
            >
              <Input
                id={key}
                name={key}
                type="number"
                inputMode="numeric"
                className="font-data"
                value={form[key]}
                onChange={(event) =>
                  set(key, Number(event.target.value) as never)
                }
                aria-invalid={errors[key] ? true : undefined}
              />
            </Field>
          ))}
        </div>
      </div>

      <div className="mt-8 flex items-center gap-3 border-t pt-6">
        <Button type="submit" disabled={!dirty || saving}>
          {saving && <RiLoader4Line className="animate-spin" />}
          Save changes
        </Button>
        {dirty && !saving && (
          <button
            type="button"
            onClick={() => setForm(zone.soa)}
            className="rounded text-sm text-muted-foreground underline-offset-4 hover:text-foreground hover:underline"
          >
            Discard
          </button>
        )}
      </div>
    </fetcher.Form>
  );
}

function Field({
  label,
  htmlFor,
  hint,
  error,
  children,
}: {
  label: string
  htmlFor: string
  hint?: string
  error?: string
  children: React.ReactNode
}) {
  return (
    <div className="space-y-2">
      <Label htmlFor={htmlFor}>{label}</Label>
      {children}
      {error ? (
        <p className="text-sm text-destructive">{error}</p>
      ) : hint ? (
        <p className="text-xs leading-relaxed text-muted-foreground">{hint}</p>
      ) : null}
    </div>
  );
}
