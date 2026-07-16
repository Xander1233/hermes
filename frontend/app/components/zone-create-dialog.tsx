import { useState } from "react";
import { useFetcher, useNavigate, useRevalidator } from "react-router";
import {
  RiAddLine,
  RiCheckboxCircleFill,
  RiLoader4Line,
} from "@remixicon/react";

import { Button } from "~/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "~/components/ui/dialog";
import { Input } from "~/components/ui/input";
import { Label } from "~/components/ui/label";
import { NameserverSetup } from "~/components/nameserver-setup";

import type { clientAction } from "~/routes/zones";

export function ZoneCreateDialog() {
  const navigate = useNavigate();
  const revalidator = useRevalidator();

  const [open, setOpen] = useState(false);
  const [apex, setApex] = useState("");

  const [session, setSession] = useState(0);
  const fetcher = useFetcher<typeof clientAction>({
    key: `zone-create-${session}`,
  });

  const submitting = fetcher.state !== "idle";
  const created = fetcher.data?.ok ? fetcher.data.data : null;
  const error = fetcher.data && !fetcher.data.ok ? (fetcher.data.fieldErrors.apex?.[0] ?? fetcher.data.message) : null;

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        setOpen(next);

        if (next) {
          setSession((count) => count + 1);
          setApex("");
        }
      }}
    >
      <DialogTrigger render={<Button />}>
        <RiAddLine />
        Add zone
      </DialogTrigger>

      <DialogContent className="sm:max-w-lg">
        {created ? (
          <>
            <DialogHeader>
              <DialogTitle className="flex items-center gap-2">
                <RiCheckboxCircleFill
                  aria-hidden="true"
                  className="size-4 shrink-0 text-success"
                />
                <span className="font-data">{created.apex}</span> is ready
              </DialogTitle>
              <DialogDescription>
                Point your registrar at these two nameservers and the zone goes live. You can add records before that happens.
              </DialogDescription>
            </DialogHeader>

            <NameserverSetup
              zone={created}
              onDelegated={() => revalidator.revalidate()}
            />

            <DialogFooter>
              <Button variant="outline" onClick={() => setOpen(false)}>
                Later
              </Button>
              <Button onClick={() => navigate(`/zones/${created.id}`)}>
                Add records
              </Button>
            </DialogFooter>
          </>
        ) : (
          <fetcher.Form method="post">
            <DialogHeader>
              <DialogTitle>Add a zone</DialogTitle>
              <DialogDescription>
                Enter the domain you want Hermes to be authoritative for.
              </DialogDescription>
            </DialogHeader>

            <div className="my-6 space-y-2">
              <Label htmlFor="apex">Domain</Label>
              <Input
                id="apex"
                name="apex"
                className="font-data"
                value={apex}
                onChange={(event) => setApex(event.target.value)}
                placeholder="example.com"
                autoFocus
                autoComplete="off"
                spellCheck={false}
                aria-invalid={error ? true : undefined}
              />
              {error ? (
                <p className="text-sm text-destructive">{error}</p>
              ) : (
                <p className="text-sm text-muted-foreground">
                  The apex, without <code className="font-data">www</code> — subdomains are records inside the zone.
                </p>
              )}
            </div>

            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => setOpen(false)}
              >
                Cancel
              </Button>
              <Button type="submit" disabled={submitting || apex.trim() === ""}>
                {submitting && <RiLoader4Line className="animate-spin" />}
                Create zone
              </Button>
            </DialogFooter>
          </fetcher.Form>
        )}
      </DialogContent>
    </Dialog>
  );
}
