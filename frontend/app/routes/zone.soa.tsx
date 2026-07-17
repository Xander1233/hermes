import { useOutletContext } from "react-router";

import { api, type Soa } from "~/lib/api";
import { runMutation } from "~/lib/mutation";
import { SoaForm } from "~/components/soa-form";

import type { Route } from "./+types/zone.soa";
import type { ZoneContext } from "./zone-layout";

export async function clientAction({
  request,
  params,
}: Route.ClientActionArgs) {
  const form = await request.formData();

  const soa: Soa = {
    mname: String(form.get("mname") ?? ""),
    rname: String(form.get("rname") ?? ""),
    serial: Number(form.get("serial") ?? 0),
    refresh: Number(form.get("refresh") ?? 0),
    retry: Number(form.get("retry") ?? 0),
    expire: Number(form.get("expire") ?? 0),
    minimum: Number(form.get("minimum") ?? 0),
    ttl: Number(form.get("ttl") ?? 0),
  };

  return runMutation(() => api.updateSoa(params.zoneId, soa));
}

export default function ZoneSoa() {
  const { zone } = useOutletContext<ZoneContext>();
  return <SoaForm zone={zone} />;
}
