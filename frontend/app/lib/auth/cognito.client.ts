import {
  AuthenticationDetails,
  CognitoUser,
  CognitoUserAttribute,
  CognitoUserPool,
  CognitoUserSession,
} from "amazon-cognito-identity-js";

import { config } from "~/lib/config";
import {
  clearSession,
  loadSession,
  saveSession,
  type Session,
  type SignInMethod,
} from "~/lib/auth/session";

const VERIFIER_KEY = "hermes.pkce_verifier";
const STATE_KEY = "hermes.oauth_state";

const TOTP_ISSUER = "Hermes DNS";

export interface Challenge {
  readonly user: CognitoUser;
}

export type MfaType = "SOFTWARE_TOKEN_MFA" | "SMS_MFA";

export type SignInResult =
  | { status: "signedIn"; session: Session }
  | { status: "newPasswordRequired"; challenge: Challenge }
  | { status: "mfaRequired"; challenge: Challenge; mfaType: MfaType };

export interface TokenResponse {
  id_token: string;
  access_token: string;
  refresh_token?: string;
  expires_in: number;
}

export class AuthError extends Error {
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = "AuthError";
    this.code = code;
  }
}

let pool: CognitoUserPool | undefined;

function userPool(): CognitoUserPool {
  pool ??= new CognitoUserPool({
    UserPoolId: config.cognito.userPoolId,
    ClientId: config.cognito.clientId,
  });

  return pool;
}

function newUser(email: string): CognitoUser {
  return new CognitoUser({ Username: email, Pool: userPool() });
}

function toSession(result: CognitoUserSession, method: SignInMethod): Session {
  const idToken = result.getIdToken();
  const claims = idToken.payload as Record<string, unknown>;

  return {
    idToken: idToken.getJwtToken(),
    accessToken: result.getAccessToken().getJwtToken(),
    refreshToken: result.getRefreshToken()?.getToken() || undefined,
    // Cognito reports the expiry in seconds; the rest of the app works in milliseconds.
    expiresAt: idToken.getExpiration() * 1000,
    sub: String(claims.sub ?? ""),
    email: String(claims.email ?? ""),
    method,
  };
}

function persist(result: CognitoUserSession, method: SignInMethod): Session {
  const session = toSession(result, method);
  saveSession(session);

  return session;
}

/** Decodes JWT without verification. Backend verifies the authenticity of the JWT */
function claimsOf(token: string): Record<string, unknown> {
  const [, payload] = token.split(".");
  if (!payload) return {};

  try {
    return JSON.parse(atob(payload.replace(/-/g, "+").replace(/_/g, "/")));
  } catch {
    return {};
  }
}

export function adoptOauthTokens(
  tokens: TokenResponse,
  existingRefreshToken?: string
): Session {
  const claims = claimsOf(tokens.id_token);

  const session: Session = {
    idToken: tokens.id_token,
    accessToken: tokens.access_token,
    refreshToken: tokens.refresh_token ?? existingRefreshToken,
    expiresAt: Date.now() + tokens.expires_in * 1000,
    sub: String(claims.sub ?? ""),
    email: String(claims.email ?? ""),
    method: "oauth",
  };

  saveSession(session);
  
  return session;
}

function toAuthError(error: unknown): AuthError {
  const code = (error as { code?: string; name?: string })?.code ?? (error as Error)?.name ?? "UnknownError";
  const original = (error as Error)?.message ?? "Sign-in failed.";

  switch (code) {
    case "NotAuthorizedException":
    case "UserNotFoundException":
      return new AuthError(code, "Incorrect email or password.");

    case "UserNotConfirmedException":
      return new AuthError(code, "This account has not been confirmed yet.");

    case "PasswordResetRequiredException":
      return new AuthError(
        code,
        "Your password must be reset before you can sign in."
      );

    case "CodeMismatchException":
      return new AuthError(code, "That code is not correct.");

    case "ExpiredCodeException":
      return new AuthError(code, "That code has expired. Request a new one.");

    case "LimitExceededException":
    case "TooManyRequestsException":
    case "TooManyFailedAttemptsException":
      return new AuthError(
        code,
        "Too many attempts. Wait a moment and try again."
      );
      
    case "InvalidPasswordException":
    case "InvalidParameterException":
      return new AuthError(code, original);

    default:
      return new AuthError(code, original);
  };
}

export function signIn(email: string, password: string): Promise<SignInResult> {
  const user = newUser(email);
  const details = new AuthenticationDetails({
    Username: email,
    Password: password,
  });

  return new Promise((resolve, reject) => {
    user.authenticateUser(details, {
      onSuccess: (result) => resolve({ status: "signedIn", session: persist(result, "srp") }),

      onFailure: (error) => reject(toAuthError(error)),

      newPasswordRequired: () => resolve({ status: "newPasswordRequired", challenge: { user } }),

      mfaRequired: () => resolve({
          status: "mfaRequired",
          challenge: { user },
          mfaType: "SMS_MFA",
        }),

      totpRequired: () => resolve({
          status: "mfaRequired",
          challenge: { user },
          mfaType: "SOFTWARE_TOKEN_MFA",
        }),
    });
  });
}

export function completeNewPassword(
  challenge: Challenge,
  newPassword: string
): Promise<SignInResult> {
  return new Promise((resolve, reject) => {
    challenge.user.completeNewPasswordChallenge(
      newPassword,
      {},
      {
        onSuccess: (result) => resolve({ status: "signedIn", session: persist(result, "srp") }),

        onFailure: (error) => reject(toAuthError(error)),

        mfaRequired: () => resolve({ status: "mfaRequired", challenge, mfaType: "SMS_MFA" }),

        totpRequired: () => resolve({
            status: "mfaRequired",
            challenge,
            mfaType: "SOFTWARE_TOKEN_MFA",
          }),
      }
    );
  });
}

export function submitMfaCode(
  challenge: Challenge,
  code: string,
  mfaType: MfaType
): Promise<SignInResult> {
  return new Promise((resolve, reject) => {
    challenge.user.sendMFACode(
      code,
      {
        onSuccess: (result) => resolve({ status: "signedIn", session: persist(result, "srp") }),

        onFailure: (error) => reject(toAuthError(error)),
      },
      mfaType
    );
  });
}

export function startPasswordReset(email: string): Promise<void> {
  return new Promise((resolve, reject) => {
    newUser(email).forgotPassword({
      onSuccess: () => resolve(),

      onFailure: (error) => reject(toAuthError(error)),
    });
  });
}

export function confirmPasswordReset(
  email: string,
  code: string,
  newPassword: string
): Promise<void> {
  return new Promise((resolve, reject) => {
    newUser(email).confirmPassword(code, newPassword, {
      onSuccess: () => resolve(),

      onFailure: (error) => reject(toAuthError(error)),
    });
  });
}

function randomString(bytes = 32): string {
  const buffer = new Uint8Array(bytes);
  crypto.getRandomValues(buffer);
  return base64UrlEncode(buffer);
}

function base64UrlEncode(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

async function sha256(input: string): Promise<Uint8Array> {
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(input)
  );
  return new Uint8Array(digest);
}

export async function buildFederatedUrl(provider: string): Promise<string> {
  const verifier = randomString(64);
  const state = randomString(16);

  sessionStorage.setItem(VERIFIER_KEY, verifier);
  sessionStorage.setItem(STATE_KEY, state);

  const params = new URLSearchParams({
    response_type: "code",
    client_id: config.cognito.clientId,
    redirect_uri: config.cognito.redirectUri,
    scope: config.cognito.scopes.join(" "),
    state,
    code_challenge: base64UrlEncode(await sha256(verifier)),
    code_challenge_method: "S256",
    identity_provider: provider,
  });

  return `https://${config.cognito.domain}/oauth2/authorize?${params}`;
}

const exchanges = new Map<string, Promise<TokenResponse>>();

export function exchangeCode(
  code: string,
  state: string
): Promise<TokenResponse> {
  let exchange = exchanges.get(code);
  if (!exchange) {
    exchange = performExchange(code, state);
    exchanges.set(code, exchange);
  }

  return exchange;
}

async function performExchange(
  code: string,
  state: string
): Promise<TokenResponse> {
  const expectedState = sessionStorage.getItem(STATE_KEY);
  const verifier = sessionStorage.getItem(VERIFIER_KEY);

  sessionStorage.removeItem(STATE_KEY);
  sessionStorage.removeItem(VERIFIER_KEY);

  if (!expectedState || state !== expectedState) {
    throw new Error("Sign-in state mismatch. Please start over.");
  }

  if (!verifier) {
    throw new Error("Missing PKCE verifier. Please start over.");
  }

  const response = await fetch(
    `https://${config.cognito.domain}/oauth2/token`,
    {
      method: "POST",
      headers: { "content-type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams({
        grant_type: "authorization_code",
        client_id: config.cognito.clientId,
        code,
        redirect_uri: config.cognito.redirectUri,
        code_verifier: verifier,
      }),
    }
  );

  if (!response.ok) {
    throw new Error(await tokenErrorMessage(response));
  }

  return (await response.json()) as TokenResponse;
}

async function tokenErrorMessage(response: Response): Promise<string> {
  const known: Record<string, string> = {
    invalid_grant: "That sign-in link had already been used or expired. Please start over.",
    invalid_client: "This app is misconfigured for sign-in. Check the Cognito app client.",
  };

  try {
    const body = (await response.json()) as { error?: string };

    if (body.error && body.error in known) {
      return known[body.error];
    }
    
    if (body.error) {
      return `Sign-in failed (${body.error}).`;
    }
  } catch {}

  return `Token exchange failed (${response.status}).`;
}

export async function refreshSession(): Promise<Session | null> {
  const stored = loadSession();
  if (!stored) return null;

  return stored.method === "srp" ? refreshSrpSession() : refreshOauthSession(stored);
}

function refreshSrpSession(): Promise<Session | null> {
  const user = userPool().getCurrentUser();
  if (!user) return Promise.resolve(null);

  return new Promise((resolve) => {
    user.getSession(
      (error: Error | null, result: CognitoUserSession | null) => {
        if (error || !result?.isValid()) {
          resolve(null);
          return;
        }

        resolve(persist(result, "srp"));
      }
    );
  });
}

async function refreshOauthSession(stored: Session): Promise<Session | null> {
  if (!stored.refreshToken) return null;

  try {
    const response = await fetch(
      `https://${config.cognito.domain}/oauth2/token`,
      {
        method: "POST",
        headers: { "content-type": "application/x-www-form-urlencoded" },
        body: new URLSearchParams({
          grant_type: "refresh_token",
          client_id: config.cognito.clientId,
          refresh_token: stored.refreshToken,
        }),
      }
    );

    if (!response.ok) return null;

    return adoptOauthTokens(
      (await response.json()) as TokenResponse,
      stored.refreshToken
    );
  } catch {
    return null;
  }
}

function currentUser(): Promise<CognitoUser> {
  const user = userPool().getCurrentUser();

  if (!user) {
    return Promise.reject(
      new AuthError(
        "NoLocalUser",
        "This is managed by the provider you signed in with, not by Hermes."
      )
    );
  }

  return new Promise((resolve, reject) => {
    user.getSession(
      (error: Error | null, session: CognitoUserSession | null) => {
        if (error || !session?.isValid()) {
          reject(
            toAuthError(
              error ?? new Error("Your session has expired. Sign in again.")
            )
          );
          return;
        }

        resolve(user);
      }
    );
  });
}

export async function changePassword(
  oldPassword: string,
  newPassword: string
): Promise<void> {
  const user = await currentUser();

  return new Promise((resolve, reject) => {
    user.changePassword(oldPassword, newPassword, (error) => {
      if (error) {
        reject(toAuthError(error));
        return;
      }

      resolve();
    });
  });
}

export interface TotpEnrolment {
  secret: string;
  uri: string;
}

export async function startTotpEnrolment(): Promise<TotpEnrolment> {
  const user = await currentUser();
  const session = loadSession();
  const account = session?.email ?? "account";

  return new Promise((resolve, reject) => {
    user.associateSoftwareToken({
      associateSecretCode: (secret: string) => {
        const label = encodeURIComponent(`${TOTP_ISSUER}:${account}`);
        const issuer = encodeURIComponent(TOTP_ISSUER);

        resolve({
          secret,
          uri: `otpauth://totp/${label}?secret=${secret}&issuer=${issuer}`,
        });
      },

      onFailure: (error) => reject(toAuthError(error)),
    });
  });
}

export async function confirmTotpEnrolment(code: string): Promise<void> {
  const user = await currentUser();

  await new Promise<void>((resolve, reject) => {
    user.verifySoftwareToken(code, "Authenticator", {
      onSuccess: () => resolve(),

      onFailure: (error) => reject(toAuthError(error)),
    });
  });

  await setTotpPreference(user, true);
}

export async function disableMfa(): Promise<void> {
  await setTotpPreference(await currentUser(), false);
}

function setTotpPreference(user: CognitoUser, enabled: boolean): Promise<void> {
  return new Promise((resolve, reject) => {
    user.setUserMfaPreference(
      null,
      { PreferredMfa: enabled, Enabled: enabled },
      (error) => {
        if (error) {
          reject(toAuthError(error));
          return;
        }

        resolve();
      }
    );
  });
}


export async function changeEmail(newEmail: string): Promise<void> {
  const user = await currentUser();
  const attributes = [
    new CognitoUserAttribute({ Name: "email", Value: newEmail }),
  ];

  return new Promise((resolve, reject) => {
    user.updateAttributes(attributes, (error) => {
      if (error) {
        reject(toAuthError(error));
        return;
      }

      resolve();
    });
  });
}

export async function confirmEmailChange(code: string): Promise<void> {
  const user = await currentUser();

  return new Promise((resolve, reject) => {
    user.verifyAttribute("email", code, {
      onSuccess: () => resolve(),

      onFailure: (error) => reject(toAuthError(error)),
    });
  });
}

export async function signOutEverywhere(): Promise<void> {
  const session = loadSession();
  if (!session) return;

  await fetch(`https://cognito-idp.${config.cognito.region}.amazonaws.com/`, {
    method: "POST",
    headers: {
      "content-type": "application/x-amz-json-1.1",
      "x-amz-target": "AWSCognitoIdentityProviderService.GlobalSignOut",
    },
    body: JSON.stringify({ AccessToken: session.accessToken }),
  });

  signOutLocal();
}

export function signOutLocal(): void {
  userPool().getCurrentUser()?.signOut();
  clearSession();
}

export function signInWithMockUser(): Session {
  const session: Session = {
    idToken: "mock.id.token",
    accessToken: "mock.access.token",
    expiresAt: Date.now() + 12 * 60 * 60 * 1000,
    sub: "mock-user-0001",
    email: "dev@hermes.local",
    method: "srp",
  };

  saveSession(session);

  return session;
}
