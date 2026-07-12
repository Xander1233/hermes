function env(key: string, fallback: string): string {
  const value = import.meta.env[key as keyof ImportMetaEnv] as string | undefined;
  return value && value.length > 0 ? value : fallback;
}

export const config = {
  /**
   * When true, every API call is served by the in-memory mock in `~/lib/api/mock` instead of hitting the management backend.
   */
  useMockApi: env("VITE_USE_MOCK_API", "true") !== "false",

  /** Base URL of the management backend (the C# service). */
  apiBaseUrl: env("VITE_API_BASE_URL", "http://localhost:5080/api/v1"),

  cognito: {
    region: env("VITE_COGNITO_REGION", "REPLACE_ME_REGION"),
    userPoolId: env("VITE_COGNITO_USER_POOL_ID", "REPLACE_ME_USER_POOL_ID"),
    clientId: env("VITE_COGNITO_CLIENT_ID", "REPLACE_ME_CLIENT_ID"),
    domain: env("VITE_COGNITO_DOMAIN", "REPLACE_ME.auth.REGION.amazoncognito.com"),
    redirectUri: env("VITE_COGNITO_REDIRECT_URI", "http://localhost:5173/auth/callback"),
    logoutUri: env("VITE_COGNITO_LOGOUT_URI", "http://localhost:5173/login"),
    scopes: env("VITE_COGNITO_SCOPES", "openid email profile").split(/\s+/),
  },
} as const;

export const cognitoConfigured = !Object.values(config.cognito)
  .filter((value): value is string => typeof value === "string")
  .some((value) => value.includes("REPLACE_ME"));
