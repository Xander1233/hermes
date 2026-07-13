import {
  RiAppleFill,
  RiGoogleFill,
  RiShieldKeyholeLine,
} from "@remixicon/react";

const KNOWN: Record<string, { label: string; Icon: typeof RiGoogleFill }> = {
  Google: { label: "Google", Icon: RiGoogleFill },
  SignInWithApple: { label: "Apple", Icon: RiAppleFill },
};

export const NATIVE_PROVIDER = "COGNITO";

export function providerLabel(name: string): string {
  return KNOWN[name]?.label ?? name;
}

export function ProviderIcon({ name }: { name: string }) {
  const Icon = KNOWN[name]?.Icon ?? RiShieldKeyholeLine;
  return <Icon />;
}

export function socialProviders(providers: string[]): string[] {
  return providers.filter((provider) => provider !== NATIVE_PROVIDER);
}
