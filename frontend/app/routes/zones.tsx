import { Link, useLoaderData } from "react-router";
import { RiArrowRightSLine, RiGlobalLine } from "@remixicon/react";

import { api, type Zone, type ZoneStatus } from "~/lib/api";
import { runMutation } from "~/lib/mutation";
import { absoluteTime, plural, relativeTime } from "~/lib/format";
import { EmptyState, PageHeader, StatusPill } from "~/components/app-shell";
import { ZoneCreateDialog } from "~/components/zone-create-dialog";
import { Skeleton } from "~/components/ui/skeleton";
import type { Crumb } from "~/components/app-shell";

import type { Route } from "./+types/zones";

export function meta() {
  return [{ title: "Zones — Hermes DNS" }];
}

export const handle = {
  crumb: (): Crumb[] => [{ label: "Zones" }],
};

export async function clientLoader() {
  return { zones: await api.listZones() };
}

export async function clientAction({ request }: Route.ClientActionArgs) {
  const form = await request.formData();
  const apex = String(form.get("apex") ?? "").trim();

  return runMutation(() => api.createZone({ apex }));
}

export function HydrateFallback() {
  return (
    <div className="space-y-8">
      <Skeleton className="h-9 w-32" />
      <div className="space-y-px rounded-xl border p-1">
        {[0, 1, 2].map((row) => (
          <Skeleton key={row} className="h-16 w-full rounded-lg" />
        ))}
      </div>
    </div>
  );
}

const STATUS: Record<ZoneStatus, { label: string; tone: "active" | "pending" | "error" }> = {
  active: { label: "Live", tone: "active" },
  pending: { label: "Awaiting delegation", tone: "pending" },
  error: { label: "Error", tone: "error" },
};

export default function Zones() {
  const { zones } = useLoaderData<typeof clientLoader>();

  const pending = zones.filter((zone) => zone.status === "pending").length;

  return (
    <>
      <PageHeader
        title="Zones"
        description={
          zones.length === 0
            ? undefined
            : pending > 0
              ? `${plural(zones.length, "zone")} · ${pending} still waiting on a registrar change.`
              : `${plural(zones.length, "zone")}, all delegated and answering.`
        }
        actions={<ZoneCreateDialog />}
      />

      {zones.length === 0 ? (
        <EmptyState
          icon={<RiGlobalLine className="size-7" />}
          title="No zones yet"
          description={
            <>
              Add a domain and Hermes becomes authoritative for it. You can fill
              in records straight away — they go live the moment your registrar
              points at us.
            </>
          }
          action={<ZoneCreateDialog />}
        />
      ) : (
        <div className="overflow-hidden rounded-xl border">
          <div
            aria-hidden="true"
            className="hidden grid-cols-[minmax(0,1fr)_7rem_6rem_8rem_1.5rem] items-center gap-4 border-b bg-surface-2 px-4 py-2 text-xs text-muted-foreground sm:grid"
          >
            <span>Zone</span>
            <span className="text-right">Records</span>
            <span className="text-right">Version</span>
            <span className="text-right">Updated</span>
            <span />
          </div>

          <ul className="divide-y">
            {zones.map((zone, index) => (
              <li key={zone.id}>
                <ZoneRow zone={zone} index={index} />
              </li>
            ))}
          </ul>
        </div>
      )}
    </>
  );
}

function ZoneRow({ zone, index }: { zone: Zone; index: number }) {
  const status = STATUS[zone.status];

  return (
    <Link
      to={`/zones/${zone.id}`}
      style={{ animationDelay: `${Math.min(index, 8) * 28}ms` }}
      className="animate-row-in grid grid-cols-[minmax(0,1fr)_1.5rem] items-center gap-x-4 gap-y-1 px-4 py-3.5 transition-colors hover:bg-surface-2 focus-visible:bg-surface-2 sm:grid-cols-[minmax(0,1fr)_7rem_6rem_8rem_1.5rem]"
    >
      <div className="min-w-0">
        <div className="font-data truncate text-[0.9375rem] leading-tight">
          {zone.apex}
        </div>
        <div className="mt-1 flex items-center gap-2">
          <StatusPill tone={status.tone}>{status.label}</StatusPill>
          {zone.statusMessage && (
            <span className="truncate text-xs text-muted-foreground">
              {zone.statusMessage}
            </span>
          )}
          <span className="text-xs text-muted-foreground sm:hidden">
            · {plural(zone.recordCount, "record")}
          </span>
        </div>
      </div>

      <div className="hidden text-right text-sm text-muted-foreground sm:block">
        {zone.recordCount}
      </div>

      <div
        className="font-data hidden text-right text-sm text-muted-foreground sm:block"
        title="Bumped on every change; the DNS fleet refetches when it moves"
      >
        v{zone.version}
      </div>

      <div
        className="hidden truncate text-right text-sm text-muted-foreground sm:block"
        title={absoluteTime(zone.updatedAt)}
      >
        {relativeTime(zone.updatedAt)}
      </div>

      <RiArrowRightSLine
        aria-hidden="true"
        className="size-4 justify-self-end text-muted-foreground/60"
      />
    </Link>
  );
}
