import { useRef, useState } from "react";
import { redirect, useLoaderData, useNavigate } from "react-router";
import {
  RiArrowLeftLine,
  RiArrowRightLine,
  RiLoader4Line,
} from "@remixicon/react";

import { api } from "~/lib/api";
import {
  buildFederatedUrl,
  completeNewPassword,
  confirmPasswordReset,
  signIn,
  signInWithMockUser,
  startPasswordReset,
  submitMfaCode,
  type Challenge,
  type MfaType,
  type SignInResult,
} from "~/lib/auth/cognito.client";
import { loadSession } from "~/lib/auth/session";
import { ProviderIcon, providerLabel, socialProviders } from "~/lib/providers";
import { cognitoConfigured } from "~/lib/config";
import { LoginCover } from "~/components/login-cover";
import { Button } from "~/components/ui/button";
import { Input } from "~/components/ui/input";
import { Label } from "~/components/ui/label";

export function meta() {
  return [{ title: "Sign in — Hermes DNS" }];
}

export async function clientLoader() {
  if (loadSession()) throw redirect("/");

  const providers = await api.listAuthProviders().catch(() => [] as string[]);
  return { providers };
}

export function HydrateFallback() {
  return null;
}

type Step =
  | { name: "credentials" }
  | { name: "newPassword" }
  | { name: "mfa"; mfaType: MfaType }
  | { name: "forgotRequest" }
  | { name: "forgotConfirm" };

export default function Login() {
  const { providers } = useLoaderData<typeof clientLoader>();
  const navigate = useNavigate();

  const [step, setStep] = useState<Step>({ name: "credentials" });
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [code, setCode] = useState("");
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const challenge = useRef<Challenge | null>(null);

  const social = socialProviders(providers);

  async function run(action: () => Promise<void>) {
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Something went wrong.");
    } finally {
      setBusy(false);
    }
  }

  function advance(result: SignInResult) {
    if (result.status === "signedIn") {
      navigate("/");
      return;
    }

    challenge.current = result.challenge;
    setNotice(null);
    setStep(
      result.status === "mfaRequired"
        ? { name: "mfa", mfaType: result.mfaType }
        : { name: "newPassword" }
    );
  }

  function requireChallenge(): Challenge {
    const pending = challenge.current;
    if (!pending) throw new Error("That step expired. Please sign in again.");
    return pending;
  }

  return (
    <main className="min-h-svh lg:grid lg:grid-cols-2">
      <LoginCover />

      <div className="flex min-h-svh flex-col justify-center px-6 py-16">
        <div className="mx-auto w-full max-w-[26rem]">
          <svg
            viewBox="0 0 16 16"
            fill="none"
            aria-hidden="true"
            className="size-6 text-primary-ink"
          >
            <circle cx="3.75" cy="8" r="2.25" fill="currentColor" />
            <path
              d="M8.25 4.6a4.6 4.6 0 0 1 0 6.8"
              stroke="currentColor"
              strokeWidth="1.6"
              strokeLinecap="round"
            />
            <path
              d="M11.6 1.9a8.6 8.6 0 0 1 0 12.2"
              stroke="currentColor"
              strokeWidth="1.6"
              strokeLinecap="round"
              opacity="0.4"
            />
          </svg>

          <h1 className="mt-6 text-2xl font-medium tracking-tight">
            {step.name === "credentials" ? "Hermes" : HEADINGS[step.name]}
          </h1>
          <p className="mt-2 max-w-[34ch] text-muted-foreground">
            {step.name === "credentials"
              ? "An authoritative DNS platform — nameserver, control plane and dashboard, built from scratch."
              : DESCRIPTIONS[step.name]}
          </p>

          {error && (
            <p
              role="alert"
              className="mt-6 rounded-lg border border-destructive/30 bg-destructive-subtle px-3.5 py-3 text-sm text-destructive"
            >
              {error}
            </p>
          )}
          {notice && !error && (
            <p className="mt-6 rounded-lg border border-primary/25 bg-primary-subtle/60 px-3.5 py-3 text-sm">
              {notice}
            </p>
          )}

          {step.name === "credentials" && (
            <>
              {social.length > 0 && (
                <>
                  <div className="mt-8 space-y-3">
                    {social.map((provider) => (
                      <SocialButton
                        key={provider}
                        provider={provider}
                        disabled={busy || !cognitoConfigured}
                        onSelect={() =>
                          run(async () => {
                            window.location.href =
                              await buildFederatedUrl(provider);
                          })
                        }
                      />
                    ))}
                  </div>
                  <Divider label="or" />
                </>
              )}

              <form
                className={social.length > 0 ? "space-y-4" : "mt-8 space-y-4"}
                onSubmit={(event) => {
                  event.preventDefault();
                  run(async () => advance(await signIn(email, password)));
                }}
              >
                <Field label="Email" htmlFor="email">
                  <Input
                    id="email"
                    name="email"
                    type="email"
                    autoComplete="username"
                    autoFocus
                    value={email}
                    onChange={(event) => setEmail(event.target.value)}
                  />
                </Field>

                <Field label="Password" htmlFor="password">
                  <Input
                    id="password"
                    name="password"
                    type="password"
                    autoComplete="current-password"
                    value={password}
                    onChange={(event) => setPassword(event.target.value)}
                  />
                </Field>

                <Button
                  type="submit"
                  size="lg"
                  className="w-full"
                  disabled={busy || !email || !password}
                >
                  {busy && <RiLoader4Line className="animate-spin" />}
                  Sign in
                  {!busy && <RiArrowRightLine data-icon="inline-end" />}
                </Button>
              </form>

              <button
                type="button"
                className="mt-4 rounded text-sm text-muted-foreground underline-offset-4 hover:text-foreground hover:underline"
                onClick={() => {
                  setError(null);
                  setNotice(null);
                  setStep({ name: "forgotRequest" });
                }}
              >
                Forgot your password?
              </button>
            </>
          )}

          {step.name === "newPassword" && (
            <StepForm
              busy={busy}
              submitLabel="Set password and continue"
              disabled={!newPassword}
              onSubmit={() =>
                run(async () =>
                  advance(
                    await completeNewPassword(requireChallenge(), newPassword)
                  )
                )
              }
            >
              <Field label="New password" htmlFor="new-password">
                <Input
                  id="new-password"
                  type="password"
                  autoComplete="new-password"
                  autoFocus
                  value={newPassword}
                  onChange={(event) => setNewPassword(event.target.value)}
                />
              </Field>
            </StepForm>
          )}

          {step.name === "mfa" && (
            <StepForm
              busy={busy}
              submitLabel="Verify"
              disabled={!code}
              onSubmit={() =>
                run(async () =>
                  advance(
                    await submitMfaCode(requireChallenge(), code, step.mfaType)
                  )
                )
              }
            >
              <Field
                label="Verification code"
                htmlFor="mfa-code"
                hint={
                  step.mfaType === "SOFTWARE_TOKEN_MFA"
                    ? "From your authenticator app."
                    : "Sent to your phone by text message."
                }
              >
                <Input
                  id="mfa-code"
                  inputMode="numeric"
                  autoComplete="one-time-code"
                  autoFocus
                  className="font-data"
                  value={code}
                  onChange={(event) => setCode(event.target.value)}
                />
              </Field>
            </StepForm>
          )}

          {step.name === "forgotRequest" && (
            <StepForm
              busy={busy}
              submitLabel="Send reset code"
              disabled={!email}
              onBack={() => setStep({ name: "credentials" })}
              onSubmit={() =>
                run(async () => {
                  await startPasswordReset(email);
                  setNotice(`We sent a code to ${email}.`);
                  setStep({ name: "forgotConfirm" });
                })
              }
            >
              <Field label="Email" htmlFor="reset-email">
                <Input
                  id="reset-email"
                  type="email"
                  autoComplete="username"
                  autoFocus
                  value={email}
                  onChange={(event) => setEmail(event.target.value)}
                />
              </Field>
            </StepForm>
          )}

          {step.name === "forgotConfirm" && (
            <StepForm
              busy={busy}
              submitLabel="Reset password"
              disabled={!code || !newPassword}
              onBack={() => setStep({ name: "forgotRequest" })}
              onSubmit={() =>
                run(async () => {
                  await confirmPasswordReset(email, code, newPassword);
                  setNotice("Password reset. Sign in with your new password.");
                  setPassword("");
                  setStep({ name: "credentials" });
                })
              }
            >
              <Field label="Reset code" htmlFor="reset-code">
                <Input
                  id="reset-code"
                  inputMode="numeric"
                  autoComplete="one-time-code"
                  autoFocus
                  className="font-data"
                  value={code}
                  onChange={(event) => setCode(event.target.value)}
                />
              </Field>
              <Field label="New password" htmlFor="reset-password">
                <Input
                  id="reset-password"
                  type="password"
                  autoComplete="new-password"
                  value={newPassword}
                  onChange={(event) => setNewPassword(event.target.value)}
                />
              </Field>
            </StepForm>
          )}

          {!cognitoConfigured && step.name === "credentials" && (
            <>
              <Divider label="not configured yet" />
              <Button
                variant="outline"
                size="lg"
                className="w-full"
                onClick={() => {
                  signInWithMockUser();
                  navigate("/");
                }}
              >
                Continue as development user
              </Button>
              <p className="pt-3 text-xs leading-relaxed text-muted-foreground">
                Set the <code className="font-data">VITE_COGNITO_*</code>{" "}
                variables in <code className="font-data">.env</code> to enable
                real sign-in. Until then the development user runs against mock
                data.
              </p>
            </>
          )}
        </div>
      </div>
    </main>
  );
}

const HEADINGS: Record<Exclude<Step["name"], "credentials">, string> = {
  newPassword: "Choose a password",
  mfa: "Two-factor authentication",
  forgotRequest: "Reset your password",
  forgotConfirm: "Enter your reset code",
};

const DESCRIPTIONS: Record<Exclude<Step["name"], "credentials">, string> = {
  newPassword:
    "Your account was created with a temporary password. Pick a new one to continue.",
  mfa: "One more step to confirm it is you.",
  forgotRequest: "We will email you a code to set a new password.",
  forgotConfirm: "Enter the code we emailed you, and the password you want.",
};

function SocialButton({
  provider,
  disabled,
  onSelect,
}: {
  provider: string
  disabled: boolean
  onSelect: () => void
}) {
  return (
    <Button
      variant="outline"
      size="lg"
      className="w-full"
      disabled={disabled}
      onClick={onSelect}
    >
      <ProviderIcon name={provider} />
      Continue with {providerLabel(provider)}
    </Button>
  );
}

function Divider({ label }: { label: string }) {
  return (
    <div className="my-6 flex items-center gap-3">
      <span className="h-px flex-1 bg-border" />
      <span className="text-xs text-muted-foreground">{label}</span>
      <span className="h-px flex-1 bg-border" />
    </div>
  );
}

function StepForm({
  busy,
  submitLabel,
  disabled,
  onSubmit,
  onBack,
  children,
}: {
  busy: boolean
  submitLabel: string
  disabled: boolean
  onSubmit: () => void
  onBack?: () => void
  children: React.ReactNode
}) {
  return (
    <form
      className="mt-8 space-y-4"
      onSubmit={(event) => {
        event.preventDefault();
        onSubmit();
      }}
    >
      {children}

      <Button
        type="submit"
        size="lg"
        className="w-full"
        disabled={busy || disabled}
      >
        {busy && <RiLoader4Line className="animate-spin" />}
        {submitLabel}
      </Button>

      {onBack && (
        <button
          type="button"
          onClick={onBack}
          className="flex items-center gap-1.5 rounded text-sm text-muted-foreground underline-offset-4 hover:text-foreground hover:underline"
        >
          <RiArrowLeftLine className="size-4" />
          Back
        </button>
      )}
    </form>
  );
}

function Field({
  label,
  htmlFor,
  hint,
  children,
}: {
  label: string
  htmlFor: string
  hint?: string
  children: React.ReactNode
}) {
  return (
    <div className="space-y-2">
      <Label htmlFor={htmlFor}>{label}</Label>
      {children}
      {hint && <p className="text-xs text-muted-foreground">{hint}</p>}
    </div>
  );
}
