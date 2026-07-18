import { Outlet, useOutletContext } from "react-router";

import { PageHeader, type Crumb } from "~/components/app-shell";

import type { AppContext } from "./app-layout";

export function meta() {
  return [{ title: "Profile — Hermes DNS" }];
}

export const handle = {
  crumb: (): Crumb[] => [{ label: "Profile" }],
};

export default function ProfileLayout() {
  const context = useOutletContext<AppContext>();

  return (
    <>
      <PageHeader
        title="Profile"
        description="How you appear, how you sign in, and where operational mail goes."
      />

      <Outlet context={context} />
    </>
  );
}
