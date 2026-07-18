import { useEffect, useState } from "react";
import { useFetcher, useOutletContext } from "react-router";
import { RiLoader4Line } from "@remixicon/react";
import { toast } from "sonner";

import { api, type UpdateProfileRequest } from "~/lib/api";
import { resetMockDatabase } from "~/lib/api/mock";
import { firstFieldErrors, runMutation } from "~/lib/mutation";
import { config } from "~/lib/config";
import { initials } from "~/components/app-shell";
import { Avatar, AvatarFallback } from "~/components/ui/avatar";
import { Button } from "~/components/ui/button";
import { Input } from "~/components/ui/input";
import { Label } from "~/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "~/components/ui/select";
import { Switch } from "~/components/ui/switch";

import type { Route } from "./+types/profile.details";
import type { AppContext } from "./app-layout";

export async function clientAction({ request }: Route.ClientActionArgs) {
  const form = await request.formData();
  const optional = (value: FormDataEntryValue | null) => String(value ?? "").trim() || null;

  const body: UpdateProfileRequest = {
    displayName: String(form.get("displayName") ?? "").trim(),
    company: optional(form.get("company")),
    timezone: String(form.get("timezone") ?? ""),
    notificationEmail: optional(form.get("notificationEmail")),
    notifyOnZoneErrors: form.get("notifyOnZoneErrors") === "on",
  };

  return runMutation(() => api.updateProfile(body));
}

// TODO Add more timezones or switch to a list provided by the backend.
const TIMEZONES = [
  "UTC",
  "Europe/Zurich",
  "Europe/London",
  "Europe/Berlin",
  "America/New_York",
  "America/Los_Angeles",
  "America/Chicago",
  "Asia/Singapore",
  "Australia/Sydney",
];

export default function ProfileDetails() {
  const { profile } = useOutletContext<AppContext>();
  const fetcher = useFetcher<typeof clientAction>();

  const initial: UpdateProfileRequest = {
    displayName: profile.displayName,
    company: profile.company ?? "",
    timezone: profile.timezone,
    notificationEmail: profile.notificationEmail ?? "",
    notifyOnZoneErrors: profile.notifyOnZoneErrors,
  };

  const [form, setForm] = useState<UpdateProfileRequest>(initial);

  const saving = fetcher.state !== "idle";
  const result = fetcher.data;
  const errors = result && !result.ok ? firstFieldErrors(result.fieldErrors) : {};
  const dirty = JSON.stringify(form) !== JSON.stringify(initial);
  const timezoneOptions = [...new Set([form.timezone, ...TIMEZONES])];

  useEffect(() => {
    if (saving || !result) {
      return;
    }

    if (result.ok) {
      toast.success("Profile saved.");
      return;
    }
    
    if (Object.keys(result.fieldErrors).length === 0) {
      toast.error(result.message);
      return;
    }
  }, [saving, result]);

  function set<K extends keyof UpdateProfileRequest>(
    key: K,
    value: UpdateProfileRequest[K]
  ) {
    setForm((previous) => ({ ...previous, [key]: value }));
  }

  return (
    <>
      <fetcher.Form method="post" className="max-w-2xl">
        <section className="space-y-5">
          <div className="flex items-center gap-3.5">
            <Avatar size="lg">
              <AvatarFallback>{initials(form.displayName)}</AvatarFallback>
            </Avatar>
            <div className="min-w-0">
              <p className="truncate font-medium">{profile.displayName}</p>
              <p className="font-data truncate text-sm text-muted-foreground">
                {profile.email}
              </p>
            </div>
          </div>

          <div className="grid gap-5 sm:grid-cols-2">
            <Field
              label="Display name"
              htmlFor="displayName"
              error={errors.displayName}
            >
              <Input
                id="displayName"
                name="displayName"
                value={form.displayName}
                onChange={(event) => set("displayName", event.target.value)}
                aria-invalid={errors.displayName ? true : undefined}
              />
            </Field>

            <Field
              label="Company"
              htmlFor="company"
              error={errors.company}
              hint="Optional."
            >
              <Input
                id="company"
                name="company"
                value={form.company ?? ""}
                onChange={(event) => set("company", event.target.value)}
              />
            </Field>

            <Field
              label="Email"
              htmlFor="email"
              hint="Managed by Cognito; change it there."
            >
              <Input
                id="email"
                value={profile.email}
                readOnly
                disabled
                className="font-data"
              />
            </Field>

            <Field
              label="Timezone"
              htmlFor="timezone"
              error={errors.timezone}
              hint="Used to render timestamps."
            >
              <Select
                value={form.timezone}
                onValueChange={(value) => set("timezone", String(value))}
                items={timezoneOptions.map((zone) => ({
                  value: zone,
                  label: zone,
                }))}
              >
                <SelectTrigger id="timezone" className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {timezoneOptions.map((zone) => (
                    <SelectItem key={zone} value={zone}>
                      {zone}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <input type="hidden" name="timezone" value={form.timezone} />
            </Field>
          </div>
        </section>

        <section className="mt-10 space-y-5 border-t pt-8">
          <div>
            <h2 className="font-medium">Notifications</h2>
            <p className="mt-1 max-w-[62ch] text-sm text-muted-foreground">
              Operational mail about your zones — delegation reminders and publish failures.
            </p>
          </div>

          <div className="max-w-sm">
            <Field
              label="Notification email"
              htmlFor="notificationEmail"
              error={errors.notificationEmail}
              hint={`Leave empty to use ${profile.email}.`}
            >
              <Input
                id="notificationEmail"
                name="notificationEmail"
                type="email"
                value={form.notificationEmail ?? ""}
                onChange={(event) => set("notificationEmail", event.target.value)}
                placeholder={profile.email}
                aria-invalid={errors.notificationEmail ? true : undefined}
              />
            </Field>
          </div>

          <div className="flex items-start justify-between gap-6">
            <div>
              <Label htmlFor="notify-errors">
                Alert me when a zone fails to publish
              </Label>
              <p className="mt-1 max-w-[54ch] text-sm text-muted-foreground">
                Sent when the DNS servers reject a change to one of your zones.
              </p>
            </div>
            <Switch
              id="notify-errors"
              checked={form.notifyOnZoneErrors}
              onCheckedChange={(checked) => set("notifyOnZoneErrors", checked)}
            />
            <input
              type="hidden"
              name="notifyOnZoneErrors"
              value={form.notifyOnZoneErrors ? "on" : "off"}
            />
          </div>
        </section>

        <div className="mt-8 flex items-center gap-3 border-t pt-6">
          <Button type="submit" disabled={!dirty || saving}>
            {saving && <RiLoader4Line className="animate-spin" />}
            Save changes
          </Button>
          {dirty && !saving && (
            <button
              type="button"
              onClick={() => setForm(initial)}
              className="rounded text-sm text-muted-foreground underline-offset-4 hover:text-foreground hover:underline"
            >
              Discard
            </button>
          )}
        </div>
      </fetcher.Form>

      {config.useMockApi && (
        <section className="mt-12 max-w-2xl border-t pt-8">
          <h2 className="font-medium">Development</h2>
          <p className="mt-1 max-w-[62ch] text-sm text-muted-foreground">
            Running against the in-memory mock backend. Its data lives in <code className="font-data">localStorage</code>, so resetting returns the seeded zones.
          </p>
          <Button
            variant="outline"
            size="sm"
            className="mt-4"
            onClick={() => {
              resetMockDatabase()
              window.location.reload()
            }}
          >
            Reset mock data
          </Button>
        </section>
      )}
    </>
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
        <p className="text-xs text-muted-foreground">{hint}</p>
      ) : null}
    </div>
  );
}
