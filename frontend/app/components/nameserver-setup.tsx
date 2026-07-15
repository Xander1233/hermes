import { useState } from "react";
import {
  RiCheckLine,
  RiFileCopyLine,
  RiLoader4Line,
  RiRefreshLine,
} from "@remixicon/react";
import { toast } from "sonner";

import { api, type DelegationCheck, type Zone } from "~/lib/api";
import { Button } from "~/components/ui/button";
import { cn } from "~/lib/utils";

export function NameserverSetup({
  zone,
  onDelegated,
}: {
  zone: Pick<Zone, "id" | "apex" | "nameservers" | "status">
  onDelegated?: () => void
}) {
  const [checking, setChecking] = useState(false);
  const [result, setResult] = useState<DelegationCheck | null>(null);

  async function check() {
    setChecking(true);

    try {
      const check = await api.checkDelegation(zone.id);
      setResult(check);

      if (!check.delegated) {
        toast.warning("Not visible yet. Registrar changes take time to propagate.");
      } else {
        toast.success(`${zone.apex} is delegated to Hermes.`);
        onDelegated?.();
      }
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Delegation check failed.");
    } finally {
      setChecking(false);
    }
  }

  return (
    <div className="space-y-4">
      <ol className="overflow-hidden rounded-lg border bg-background">
        {zone.nameservers.map((nameserver, index) => (
          <li
            key={nameserver}
            className={cn(
              "flex items-center gap-3 px-3 py-2.5",
              index > 0 && "border-t"
            )}
          >
            <span
              aria-hidden="true"
              className="font-data w-4 shrink-0 text-xs text-muted-foreground"
            >
              {index + 1}
            </span>
            <code className="font-data flex-1 truncate text-sm">
              {nameserver}
            </code>
            <CopyButton value={nameserver} label={`Copy ${nameserver}`} />
          </li>
        ))}
      </ol>

      <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
        <Button variant="outline" size="sm" onClick={check} disabled={checking}>
          {checking ? (
            <RiLoader4Line className="animate-spin" />
          ) : (
            <RiRefreshLine />
          )}
          {checking ? "Checking…" : "Check delegation"}
        </Button>

        <CopyButton
          value={zone.nameservers.join("\n")}
          label="Copy both nameservers"
          variant="text"
        />
      </div>

      {result && !result.delegated && (
        <div className="rounded-lg border bg-surface-2 px-3.5 py-3 text-sm">
          <p className="font-medium">Not delegated yet</p>
          <p className="mt-1 max-w-[64ch] text-muted-foreground">
            {result.observedNameservers.length > 0 ? (
              <>
                The parent zone still answers with{" "}
                <span className="font-data text-foreground">
                  {result.observedNameservers.join(", ")}
                </span>
                . Registrar changes propagate on the parent's TTL, often 24–48 hours.
              </>
            ) : (
              <>
                Nothing is answering for {zone.apex} at the parent zone yet. Registrar changes propagate
                on the parent's TTL, often 24–48 hours. Keep adding records meanwhile — they're already published.
              </>
            )}
          </p>
        </div>
      )}

      {result?.delegated && (
        <p className="flex items-center gap-2 text-sm text-success">
          <RiCheckLine className="size-4 shrink-0" />
          Delegation confirmed. {zone.apex} is answering from Hermes.
        </p>
      )}
    </div>
  );
}

function CopyButton({
  value,
  label,
  variant = "icon",
}: {
  value: string
  label: string
  variant?: "icon" | "text"
}) {
  const [copied, setCopied] = useState(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(value);

      setCopied(true);
      setTimeout(() => setCopied(false), 1600);
    } catch {
      toast.error("Clipboard unavailable — select and copy manually.");
    }
  }

  if (variant === "text") {
    return (
      <Button variant="ghost" size="sm" onClick={copy}>
        {copied ? <RiCheckLine className="text-success" /> : <RiFileCopyLine />}
        {copied ? "Copied" : "Copy both"}
      </Button>
    );
  }

  return (
    <Button variant="ghost" size="icon-sm" onClick={copy} aria-label={label}>
      {copied ? <RiCheckLine className="text-success" /> : <RiFileCopyLine />}
    </Button>
  );
}
