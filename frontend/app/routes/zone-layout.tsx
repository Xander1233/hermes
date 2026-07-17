import { useEffect, useState } from "react";
import {
  Outlet,
  useFetcher,
  useLoaderData,
  useRevalidator,
  type ShouldRevalidateFunctionArgs,
} from "react-router";
import { RiAddLine, RiDeleteBinLine, RiMore2Fill } from "@remixicon/react";
import { toast } from "sonner";

import { api, type DnsRecord, type ZoneDetail } from "~/lib/api";
import { config } from "~/lib/config";
import { markDelegated } from "~/lib/api/mock";
import { absoluteTime, plural, relativeTime } from "~/lib/format";
import { PageHeader, StatusPill, type Crumb } from "~/components/app-shell";
import { NameserverSetup } from "~/components/nameserver-setup";
import { RecordDialog } from "~/components/record-dialog";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "~/components/ui/alert-dialog";
import { Button } from "~/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "~/components/ui/dropdown-menu";
import { Skeleton } from "~/components/ui/skeleton";

import type { Route } from "./+types/zone-layout";
import type { clientAction as recordsAction } from "./zone.records";

/** Position of the zone id in `/zones/<id>/<section>`. */
const ZONE_ID_SEGMENT = 2;

export interface ZoneContext {
  zone: ZoneDetail;
  refresh: () => void;
  openRecordDialog: (record?: DnsRecord) => void;
}

export function meta({ data }: Route.MetaArgs) {
  return [{ title: `${data?.zone.apex ?? "Zone"} — Hermes DNS` }];
}

export const handle = {
  crumb: (data: unknown): Crumb[] => [
    { label: "Zones", to: "/" },
    { label: (data as { zone?: { apex?: string } })?.zone?.apex ?? "Zone" },
  ],
}

export async function clientLoader({ params }: Route.ClientLoaderArgs) {
  return { zone: await api.getZone(params.zoneId) };
}

function zoneIdOf(url: URL): string | undefined {
  return url.pathname.split("/")[ZONE_ID_SEGMENT];
}

export function shouldRevalidate({
  currentUrl,
  nextUrl,
}: ShouldRevalidateFunctionArgs) {
  if (zoneIdOf(currentUrl) !== zoneIdOf(nextUrl)) return true;

  return currentUrl.href === nextUrl.href;
}

export function HydrateFallback() {
  return (
    <div className="space-y-8">
      <Skeleton className="h-9 w-72" />
      <Skeleton className="h-64 w-full rounded-xl" />
    </div>
  );
}

export default function ZoneLayout() {
  const { zone } = useLoaderData<typeof clientLoader>();
  const revalidator = useRevalidator();
  const deleteZone = useFetcher<typeof recordsAction>();

  const [dialogOpen, setDialogOpen] = useState(false);
  const [editing, setEditing] = useState<DnsRecord | undefined>(undefined);
  const [confirmDelete, setConfirmDelete] = useState(false);

  const refresh = () => revalidator.revalidate();
  const isPending = zone.status === "pending";
  const deleting = deleteZone.state !== "idle";

  function openRecordDialog(record?: DnsRecord) {
    setEditing(record);
    requestAnimationFrame(() => setDialogOpen(true));
  }

  useEffect(() => {
    if (!deleting && deleteZone.data && !deleteZone.data.ok) {
      toast.error(deleteZone.data.message);
    }
  }, [ deleting, deleteZone.data ]);

  const context: ZoneContext = { zone, refresh, openRecordDialog };

  return (
    <>
      <PageHeader
        title={<span className="font-data">{zone.apex}</span>}
        description={
          <span className="flex flex-wrap items-center gap-x-3 gap-y-1">
            <StatusPill tone={isPending ? "pending" : "active"}>
              {isPending ? "Awaiting delegation" : "Live"}
            </StatusPill>
            <span aria-hidden="true" className="text-muted-foreground">
              ·
            </span>
            <span>{plural(zone.recordCount, "record")}</span>
            <span aria-hidden="true" className="text-muted-foreground">
              ·
            </span>
            <span
              className="font-data"
              title="Bumped on every change; the DNS fleet refetches when it moves"
            >
              v{zone.version}
            </span>
            <span aria-hidden="true" className="text-muted-foreground">
              ·
            </span>
            <span title={absoluteTime(zone.updatedAt)}>
              updated {relativeTime(zone.updatedAt)}
            </span>
          </span>
        }
        actions={
          <>
            <Button onClick={() => openRecordDialog()}>
              <RiAddLine />
              Add record
            </Button>

            <DropdownMenu>
              <DropdownMenuTrigger
                render={
                  <Button
                    variant="outline"
                    size="icon"
                    aria-label="More zone actions"
                  />
                }
              >
                <RiMore2Fill />
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuItem
                  variant="destructive"
                  onClick={() =>
                    requestAnimationFrame(() => setConfirmDelete(true))
                  }
                >
                  <RiDeleteBinLine />
                  Delete zone
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </>
        }
      />

      {isPending && (
        <section className="mb-8 rounded-xl border border-primary/25 bg-primary-subtle/50 p-5 sm:p-6">
          <h2 className="text-base font-medium">
            One step left: point {zone.apex} at these nameservers
          </h2>
          <p className="mt-1.5 max-w-[64ch] text-sm text-muted-foreground">
            Until your registrar is updated, queries still go to your old provider.
            Records you add here are already published and start answering the moment delegation lands.
          </p>

          <div className="mt-5">
            <NameserverSetup zone={zone} onDelegated={refresh} />
          </div>

          {config.useMockApi && (
            <p className="mt-4 border-t border-primary/15 pt-3 text-xs text-muted-foreground">
              Mock only:{" "}
              <button
                type="button"
                className="rounded underline underline-offset-2 hover:text-foreground"
                onClick={() => {
                  markDelegated(zone.id);
                  toast.info("Pretending the registrar was updated. Check again.");
                }}
              >
                simulate the registrar update
              </button>
            </p>
          )}
        </section>
      )}

      <Outlet context={context} />

      <RecordDialog
        zone={zone}
        record={editing}
        open={dialogOpen}
        onOpenChange={setDialogOpen}
      />

      <AlertDialog open={confirmDelete} onOpenChange={setConfirmDelete}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              Delete <span className="font-data">{zone.apex}</span>?
            </AlertDialogTitle>
            <AlertDialogDescription>
              {plural(zone.recordCount, "record")} will be removed and Hermes stops answering for this zone.
              Anything still pointed here fails to resolve. This can&apos;t be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={deleting}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              disabled={deleting}
              onClick={() =>
                deleteZone.submit(
                  { intent: "delete-zone" },
                  {
                    method: "post",
                    encType: "application/json",
                    action: `/zones/${zone.id}?index`,
                  }
                )
              }
            >
              Delete zone
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
