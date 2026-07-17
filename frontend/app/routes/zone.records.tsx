import { redirect, useLoaderData, useOutletContext } from "react-router";

import {
  api,
  type CreateRecordRequest,
  type UpdateRecordRequest,
} from "~/lib/api";
import { runMutation } from "~/lib/mutation";
import { RecordsTable } from "~/components/records-table";
import { Skeleton } from "~/components/ui/skeleton";
import type { ZoneContext } from "./zone-layout";

import type { Route } from "./+types/zone.records";

export async function clientLoader({ params }: Route.ClientLoaderArgs) {
  return { records: await api.listRecords(params.zoneId) };
}

type RecordAction =
  | { intent: "create"; body: CreateRecordRequest }
  | { intent: "update"; recordId: string; body: UpdateRecordRequest }
  | { intent: "delete"; recordId: string }
  | { intent: "delete-zone" };

export async function clientAction({
  request,
  params,
}: Route.ClientActionArgs) {
  const action = (await request.json()) as RecordAction;
  const { zoneId } = params;

  switch (action.intent) {
    case "create":
      return runMutation(() => api.createRecord(zoneId, action.body));

    case "update":
      return runMutation(() => api.updateRecord(zoneId, action.recordId, action.body));

    case "delete":
      return runMutation(() => api.deleteRecord(zoneId, action.recordId));

    case "delete-zone":
      const result = await runMutation(() => api.deleteZone(zoneId));
      return result.ok ? redirect("/") : result;
  };
}

export function HydrateFallback() {
  return <Skeleton className="h-64 w-full rounded-xl" />;
}

export default function ZoneRecords() {
  const { records } = useLoaderData<typeof clientLoader>();
  const { zone, openRecordDialog } = useOutletContext<ZoneContext>();

  return (
    <RecordsTable zone={zone} records={records} onEdit={openRecordDialog} />
  );
}
