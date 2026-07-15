import { useEffect, useState } from "react";
import { useFetcher } from "react-router";
import {
  RiDeleteBinLine,
  RiEditLine,
  RiMore2Fill,
  RiStickyNoteLine,
} from "@remixicon/react";
import { toast } from "sonner";

import { type DnsRecord, type ZoneDetail } from "~/lib/api";
import {
  displayType,
  formatRecordValue,
  formatTtl,
  isFlattenedApexCname,
  toRelativeName,
} from "~/lib/dns";
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
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "~/components/ui/tooltip";
import { cn } from "~/lib/utils";

import type { clientAction } from "~/routes/zone.records";

const ROW_GRID = "grid-cols-[auto_minmax(0,1fr)_auto] gap-x-3 gap-y-1 py-3 sm:grid-cols-[5rem_minmax(8rem,1fr)_minmax(0,2.2fr)_5rem_2rem] sm:gap-x-4 sm:gap-y-0 sm:py-2.5";
const UNFOLD = "sm:col-auto sm:row-auto sm:col-span-1";

export function RecordsTable({
  zone,
  records,
  onEdit,
}: {
  zone: ZoneDetail
  records: DnsRecord[]
  onEdit: (record: DnsRecord) => void
}) {
  const fetcher = useFetcher<typeof clientAction>();
  const [pendingDelete, setPendingDelete] = useState<DnsRecord | null>(null);

  const deleting = fetcher.state !== "idle";

  useEffect(() => {
    if (deleting || !fetcher.data) return;

    if (!fetcher.data.ok) {
      toast.error(fetcher.data.message);
      return;
    }

    toast.success("Record deleted.");
    setPendingDelete(null);
  }, [ deleting, fetcher.data ]);

  function confirmDelete() {
    if (!pendingDelete) return;

    fetcher.submit(
      { intent: "delete", recordId: pendingDelete.id },
      {
        method: "post",
        encType: "application/json",
        action: `/zones/${zone.id}?index`,
      }
    );
  }

  const hasFlattened = records.some((record) => isFlattenedApexCname(record, zone.apex));

  return (
    <>
      <div className="overflow-hidden rounded-xl border">
        <div
          aria-hidden="true"
          className={cn(
            "hidden items-center border-b bg-surface-2 px-4 py-2 text-xs text-muted-foreground sm:grid",
            ROW_GRID
          )}
        >
          <span>Type</span>
          <span>Name</span>
          <span>Value</span>
          <span className="text-right">TTL</span>
          <span />
        </div>

        {records.length === 0 ? (
          <p className="px-4 py-12 text-center text-sm text-muted-foreground">
            No records yet. Add one and it reaches the nameservers within seconds.
          </p>
        ) : (
          <ul className="divide-y">
            {records.map((record, index) => (
              <li
                key={record.id}
                style={{ animationDelay: `${Math.min(index, 10) * 22}ms` }}
                className={cn(
                  "animate-row-in grid items-center px-4 transition-colors hover:bg-surface-2",
                  ROW_GRID
                )}
              >
                <div
                  className={cn(
                    "col-start-1 row-start-1 flex min-w-0 items-center",
                    UNFOLD
                  )}
                >
                  <span className="font-data text-xs font-medium">
                    {displayType(record)}
                  </span>
                  {isFlattenedApexCname(record, zone.apex) && <FlattenedTag />}
                </div>

                <div
                  className={cn(
                    "font-data col-start-2 row-start-1 min-w-0 truncate text-sm",
                    UNFOLD
                  )}
                >
                  {record.name === zone.apex ? (
                    <span className="text-muted-foreground">{zone.apex}</span>
                  ) : (
                    <>
                      {toRelativeName(record.name, zone.apex)}
                      <span className="text-muted-foreground">
                        .{zone.apex}
                      </span>
                    </>
                  )}
                </div>

                <div
                  className={cn(
                    "col-span-2 col-start-1 row-start-2 flex min-w-0 items-center gap-1.5",
                    UNFOLD
                  )}
                >
                  <span className="font-data truncate text-sm text-muted-foreground">
                    {formatRecordValue(record)}
                  </span>
                  {record.notes && <NoteIndicator note={record.notes} />}
                </div>

                <div
                  className={cn(
                    "col-start-3 row-start-2 text-right text-xs whitespace-nowrap text-muted-foreground",
                    UNFOLD
                  )}
                >
                  {formatTtl(record.ttl)}
                </div>

                <DropdownMenu>
                  <DropdownMenuTrigger
                    render={
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        className={cn("col-start-3 row-start-1", UNFOLD)}
                        aria-label={`Actions for ${record.name} ${displayType(record)}`}
                      />
                    }
                  >
                    <RiMore2Fill />
                  </DropdownMenuTrigger>
                  <DropdownMenuContent align="end">
                    <DropdownMenuItem onClick={() => onEdit(record)}>
                      <RiEditLine />
                      Edit
                    </DropdownMenuItem>
                    <DropdownMenuItem
                      variant="destructive"
                      onClick={() =>
                        requestAnimationFrame(() => setPendingDelete(record))
                      }
                    >
                      <RiDeleteBinLine />
                      Delete
                    </DropdownMenuItem>
                  </DropdownMenuContent>
                </DropdownMenu>
              </li>
            ))}
          </ul>
        )}
      </div>

      {hasFlattened && (
        <p className="mt-3 max-w-[72ch] text-xs text-muted-foreground">
          <span className="font-medium text-foreground">Flattened</span> marks a CNAME at the zone apex. A real CNAME can't sit beside the apex's SOA and NS records,
          so Hermes stores it as an ALIAS and answers with the target's current addresses instead. Nothing to do — it behaves like the CNAME you asked for.
        </p>
      )}

      <AlertDialog
        open={pendingDelete !== null}
        onOpenChange={(open) => !open && setPendingDelete(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete this record?</AlertDialogTitle>
            <AlertDialogDescription>
              {pendingDelete && (
                <>
                  <span className="font-data text-foreground">
                    {toRelativeName(pendingDelete.name, zone.apex)}{" "}
                    {displayType(pendingDelete)}{" "}
                    {formatRecordValue(pendingDelete)}
                  </span>{" "}
                  stops resolving as soon as caches expire. This can't be undone.
                </>
              )}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={deleting}>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={confirmDelete} disabled={deleting}>
              Delete record
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}

function FlattenedTag() {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <span className="ml-1.5 shrink-0 cursor-help rounded-sm bg-primary-subtle px-1 py-px text-[0.625rem] leading-tight text-primary-ink" />
        }
      >
        flat
      </TooltipTrigger>
      <TooltipContent className="max-w-xs">
        Stored as an ALIAS. A CNAME can't exist at the apex alongside the SOA and NS records, so this is answered with the target's current A/AAAA addresses.
      </TooltipContent>
    </Tooltip>
  );
}

function NoteIndicator({ note }: { note: string }) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <span
            className="shrink-0 cursor-help text-muted-foreground"
            aria-label="Has a note"
          />
        }
      >
        <RiStickyNoteLine className="size-3.5" />
      </TooltipTrigger>
      <TooltipContent className="max-w-xs whitespace-pre-wrap">
        {note}
      </TooltipContent>
    </Tooltip>
  );
}
