import { useOutletContext } from "react-router";

import { NameserverSetup } from "~/components/nameserver-setup";
import type { ZoneContext } from "./zone-layout";

export default function ZoneNameservers() {
  const { zone, refresh } = useOutletContext<ZoneContext>();

  return (
    <div className="max-w-2xl space-y-5">
      <div>
        <h2 className="font-medium">Nameservers</h2>
        <p className="mt-1 max-w-[64ch] text-sm text-muted-foreground">
          {zone.status === "pending"
            ? "Set these at your registrar to finish delegating the zone."
            : "These answer for the zone. Your registrar already points at them."}
        </p>
      </div>

      <NameserverSetup zone={zone} onDelegated={refresh} />
    </div>
  );
}
