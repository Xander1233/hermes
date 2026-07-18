import { useEffect, useState } from "react";
import { useFetcher, useNavigate } from "react-router";
import { RiLoader4Line, RiShieldCheckLine } from "@remixicon/react";
import { toast } from "sonner";

import {
  changeEmail,
  changePassword,
  confirmEmailChange,
  confirmTotpEnrolment,
  disableMfa,
  signOutEverywhere,
  startTotpEnrolment,
} from "~/lib/auth/cognito.client";
import { loadSession, type SignInMethod } from "~/lib/auth/session";
import { runMutation } from "~/lib/mutation";
import { Button } from "~/components/ui/button";
import { Input } from "~/components/ui/input";
import { Label } from "~/components/ui/label";

import type { Route } from "./+types/profile.security";

type SecurityAction =
  | { intent: "changePassword"; currentPassword: string; newPassword: string }
  | { intent: "startTotp" }
  | { intent: "confirmTotp"; code: string }
  | { intent: "disableMfa" }
  | { intent: "changeEmail"; email: string }
  | { intent: "confirmEmail"; code: string }
  | { intent: "signOutEverywhere" };

export async function clientAction({ request }: Route.ClientActionArgs) {
  const action = (await request.json()) as SecurityAction;

  switch (action.intent) {
    case "changePassword":
      return runMutation(() =>
        changePassword(action.currentPassword, action.newPassword)
      );

    case "startTotp":
      return runMutation(() => startTotpEnrolment());

    case "confirmTotp":
      return runMutation(() => confirmTotpEnrolment(action.code));

    case "disableMfa":
      return runMutation(() => disableMfa());

    case "changeEmail":
      return runMutation(() => changeEmail(action.email));

    case "confirmEmail":
      return runMutation(() => confirmEmailChange(action.code));

    case "signOutEverywhere":
      return runMutation(() => signOutEverywhere());
  };
}

export default function ProfileSecurity() {
  const [method, setMethod] = useState<SignInMethod | null>(null);
  useEffect(() => setMethod(loadSession()?.method ?? null), []);

  if (method === "oauth") {
    return (
      <div className="max-w-2xl rounded-xl border bg-surface-2 px-4 py-3.5 text-sm">
        <p className="font-medium">Managed by your sign-in provider</p>
        <p className="mt-1 max-w-[64ch] text-muted-foreground">
          You signed in with a social account, so your password and two-factor settings live
          there rather than here. Connect a password to this account to manage them from Hermes.
        </p>
      </div>
    );
  }

  return (
    <div className="max-w-2xl">
      <PasswordSection />
      <MfaSection />
      <EmailSection />
      <SessionsSection />
    </div>
  );
}

function PasswordSection() {
  const fetcher = useFetcher<typeof clientAction>();
  const [currentPassword, setCurrentPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");

  const busy = fetcher.state !== "idle";

  useSettled(fetcher, () => {
    toast.success("Password changed.")
    setCurrentPassword("")
    setNewPassword("")
  });

  return (
    <Section
      title="Password"
      description="Changing this signs you out of nothing — use “Sign out everywhere” below for that."
    >
      <form
        className="space-y-4"
        onSubmit={(event) => {
          event.preventDefault();
          submit(fetcher, {
            intent: "changePassword",
            currentPassword,
            newPassword,
          });
        }}
      >
        <Field label="Current password" htmlFor="current-password">
          <Input
            id="current-password"
            type="password"
            autoComplete="current-password"
            value={currentPassword}
            onChange={(event) => setCurrentPassword(event.target.value)}
          />
        </Field>

        <Field label="New password" htmlFor="new-password">
          <Input
            id="new-password"
            type="password"
            autoComplete="new-password"
            value={newPassword}
            onChange={(event) => setNewPassword(event.target.value)}
          />
        </Field>

        <Button
          type="submit"
          disabled={busy || !currentPassword || !newPassword}
        >
          {busy && <RiLoader4Line className="animate-spin" />}
          Change password
        </Button>
      </form>
    </Section>
  );
}

function MfaSection() {
  const fetcher = useFetcher<typeof clientAction>();
  const [secret, setSecret] = useState<string | null>(null);
  const [code, setCode] = useState("");

  const busy = fetcher.state !== "idle";

  useEffect(() => {
    if (busy || !fetcher.data) return;

    if (!fetcher.data.ok) {
      toast.error(fetcher.data.message);
      return;
    }

    const enrolment = fetcher.data.data as { secret: string } | undefined;
    if (enrolment?.secret) {
      setSecret(enrolment.secret);
      return;
    }

    setSecret(null);
    setCode("");
    toast.success("Two-factor settings updated.");
  }, [ busy, fetcher.data ]);

  return (
    <Section
      title="Two-factor authentication"
      description="An authenticator app generates a code that is asked for after your password."
    >
      {secret ? (
        <div className="space-y-4">
          <div className="rounded-lg border bg-surface-2 px-3.5 py-3">
            <p className="text-sm font-medium">
              Add this key to your authenticator
            </p>
            <code className="font-data mt-2 block text-sm break-all select-all">
              {secret}
            </code>
            <p className="mt-2 text-xs text-muted-foreground">
              Then enter the six-digit code it shows, to prove it worked before we turn it on.
            </p>
          </div>

          <form
            className="space-y-4"
            onSubmit={(event) => {
              event.preventDefault();
              submit(fetcher, { intent: "confirmTotp", code });
            }}
          >
            <Field label="Code from your app" htmlFor="totp-code">
              <Input
                id="totp-code"
                inputMode="numeric"
                autoComplete="one-time-code"
                className="font-data"
                value={code}
                onChange={(event) => setCode(event.target.value)}
              />
            </Field>

            <div className="flex items-center gap-3">
              <Button type="submit" disabled={busy || !code}>
                {busy && <RiLoader4Line className="animate-spin" />}
                Turn on two-factor
              </Button>
              <button
                type="button"
                className="rounded text-sm text-muted-foreground underline-offset-4 hover:text-foreground hover:underline"
                onClick={() => setSecret(null)}
              >
                Cancel
              </button>
            </div>
          </form>
        </div>
      ) : (
        <div className="flex flex-wrap gap-3">
          <Button
            variant="outline"
            disabled={busy}
            onClick={() => submit(fetcher, { intent: "startTotp" })}
          >
            {busy ? (
              <RiLoader4Line className="animate-spin" />
            ) : (
              <RiShieldCheckLine />
            )}
            Set up authenticator
          </Button>
          <Button
            variant="ghost"
            disabled={busy}
            onClick={() => submit(fetcher, { intent: "disableMfa" })}
          >
            Turn off
          </Button>
        </div>
      )}
    </Section>
  );
}

function EmailSection() {
  const fetcher = useFetcher<typeof clientAction>();
  const [email, setEmail] = useState("");
  const [code, setCode] = useState("");
  const [awaitingCode, setAwaitingCode] = useState(false);

  const busy = fetcher.state !== "idle";

  useSettled(fetcher, () => {
    if (awaitingCode) {
      toast.success("Email address confirmed.");

      setAwaitingCode(false);
      setEmail("");
      setCode("");
      return;
    }

    toast.success(`We sent a code to ${email}.`);
    setAwaitingCode(true);
  });

  return (
    <Section
      title="Email address"
      description="This is what you sign in with, so the new address has to be confirmed before it takes effect."
    >
      <form
        className="space-y-4"
        onSubmit={(event) => {
          event.preventDefault();
          submit(
            fetcher,
            awaitingCode
              ? { intent: "confirmEmail", code }
              : { intent: "changeEmail", email }
          );
        }}
      >
        {awaitingCode ? (
          <Field label="Confirmation code" htmlFor="email-code">
            <Input
              id="email-code"
              inputMode="numeric"
              autoComplete="one-time-code"
              className="font-data"
              value={code}
              onChange={(event) => setCode(event.target.value)}
            />
          </Field>
        ) : (
          <Field label="New email address" htmlFor="new-email">
            <Input
              id="new-email"
              type="email"
              value={email}
              onChange={(event) => setEmail(event.target.value)}
            />
          </Field>
        )}

        <Button
          type="submit"
          disabled={busy || (awaitingCode ? !code : !email)}
        >
          {busy && <RiLoader4Line className="animate-spin" />}
          {awaitingCode ? "Confirm address" : "Send confirmation code"}
        </Button>
      </form>
    </Section>
  );
}

function SessionsSection() {
  const fetcher = useFetcher<typeof clientAction>();
  const navigate = useNavigate();
  const busy = fetcher.state !== "idle";

  useSettled(fetcher, () => navigate("/login"));

  return (
    <Section
      title="Sessions"
      description="Signs you out on every device, including this one. Use it if you think someone else has your password."
      last
    >
      <Button
        variant="outline"
        disabled={busy}
        onClick={() => submit(fetcher, { intent: "signOutEverywhere" })}
      >
        {busy && <RiLoader4Line className="animate-spin" />}
        Sign out everywhere
      </Button>
    </Section>
  );
}

type SecurityFetcher = ReturnType<typeof useFetcher<typeof clientAction>>;

function submit(fetcher: SecurityFetcher, action: SecurityAction) {
  fetcher.submit(action, { method: "post", encType: "application/json" });
}

function useSettled(fetcher: SecurityFetcher, onSuccess: () => void) {
  const busy = fetcher.state !== "idle";
  const result = fetcher.data;

  useEffect(() => {
    if (busy || !result) return;

    if (!result.ok) {
      toast.error(result.message);
      return;
    }

    onSuccess();
  }, [ busy, result ]);
}

function Section({
  title,
  description,
  last,
  children,
}: {
  title: string
  description: string
  last?: boolean
  children: React.ReactNode
}) {
  return (
    <section
      className={
        last ? "border-t py-8" : "border-t py-8 first:border-t-0 first:pt-0"
      }
    >
      <h2 className="font-medium">{title}</h2>
      <p className="mt-1 mb-5 max-w-[64ch] text-sm text-muted-foreground">
        {description}
      </p>
      {children}
    </section>
  );
}

function Field({
  label,
  htmlFor,
  children,
}: {
  label: string
  htmlFor: string
  children: React.ReactNode
}) {
  return (
    <div className="max-w-sm space-y-2">
      <Label htmlFor={htmlFor}>{label}</Label>
      {children}
    </div>
  );
}
