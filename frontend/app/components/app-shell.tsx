import { Fragment, useState } from "react";
import { Link, useMatches, useNavigate } from "react-router";
import {
  RiLogoutBoxRLine,
  RiMenuLine,
  RiMoonLine,
  RiSunLine,
  RiUser3Line,
} from "@remixicon/react";

import type { UserProfile } from "~/lib/api";
import { config } from "~/lib/config";
import { signOutLocal } from "~/lib/auth/cognito.client";
import { Avatar, AvatarFallback } from "~/components/ui/avatar";
import { Button } from "~/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "~/components/ui/dropdown-menu";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
  SheetTrigger,
} from "~/components/ui/sheet";
import { Sidebar, SidebarNav, useSidebarSections } from "~/components/sidebar";
import { cn } from "~/lib/utils";

export interface Crumb {
  label: string;
  to?: string;
}

interface CrumbHandle {
  crumb?: (data: unknown) => Crumb[];
}

export function initials(name: string): string {
  return (
    name
      .split(/\s+/)
      .filter(Boolean)
      .slice(0, 2)
      .map((part) => part[0]?.toUpperCase() ?? "")
      .join("") || "?"
  );
}

/**
 * Hermes Logo
 */
function Mark({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 16 16"
      fill="none"
      aria-hidden="true"
      className={cn("size-4 text-primary-ink", className)}
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
  );
}

function toggleTheme() {
  const root = document.documentElement;
  const next = root.classList.contains("dark") ? "light" : "dark";

  root.classList.toggle("dark", next === "dark");
  root.style.colorScheme = next;

  try {
    localStorage.setItem("hermes-theme", next);
  } catch {}
}

export function AppShell({
  profile,
  children,
}: {
  profile: UserProfile
  children: React.ReactNode
}) {
  const navigate = useNavigate();
  const matches = useMatches();
  const [drawerOpen, setDrawerOpen] = useState(false);

  const crumbs = matches.flatMap((match) => {
    const handle = match.handle as CrumbHandle | undefined;
    return handle?.crumb?.(match.data) ?? [];
  });

  const { sections, backLink } = useSidebarSections();
  const scopeLabel = backLink ? crumbs.at(-1)?.label : undefined;

  function signOut() {
    signOutLocal();
    navigate("/login");
  }

  return (
    <div className="min-h-svh bg-background">
      <a href="#main" className="skip-link">
        Skip to content
      </a>

      <header className="sticky top-0 z-40 h-[var(--header-height)] border-b bg-background/90 backdrop-blur-sm">
        <div className="flex h-full items-center gap-2 px-3 sm:gap-3 sm:px-4">
          <Sheet open={drawerOpen} onOpenChange={setDrawerOpen}>
            <SheetTrigger
              render={
                <Button
                  variant="ghost"
                  size="icon-sm"
                  className="lg:hidden"
                  aria-label="Open navigation"
                />
              }
            >
              <RiMenuLine />
            </SheetTrigger>
            <SheetContent side="left" className="w-72 p-0 sm:max-w-72">
              <SheetHeader className="border-b px-4 py-3">
                <SheetTitle className="flex items-center gap-2 text-sm">
                  <Mark />
                  Hermes
                </SheetTitle>
                {scopeLabel && (
                  <SheetDescription className="font-data truncate text-xs">
                    {scopeLabel}
                  </SheetDescription>
                )}
              </SheetHeader>
              <SidebarNav
                sections={sections}
                backLink={backLink}
                collapsed={false}
                onNavigate={() => setDrawerOpen(false)}
              />
            </SheetContent>
          </Sheet>

          <Link
            to="/"
            className="flex shrink-0 items-center gap-2 rounded-md text-sm font-medium tracking-tight"
          >
            <Mark />
            Hermes
          </Link>

          {crumbs.length > 0 && (
            <nav
              aria-label="Breadcrumb"
              className="flex min-w-0 flex-1 items-center"
            >
              <ol className="flex min-w-0 items-center text-sm">
                {crumbs.map((crumb, index) => {
                  const last = index === crumbs.length - 1
                  return (
                    <Fragment key={`${crumb.label}-${index}`}>
                      <li
                        aria-hidden="true"
                        className={cn(
                          "shrink-0 px-2 text-muted-foreground select-none",
                          !last && "hidden sm:block"
                        )}
                      >
                        /
                      </li>
                      <li className={cn("min-w-0", !last && "hidden sm:block")}>
                        {crumb.to && !last ? (
                          <Link
                            to={crumb.to}
                            className="rounded text-muted-foreground transition-colors hover:text-foreground"
                          >
                            {crumb.label}
                          </Link>
                        ) : (
                          <span
                            className={cn(
                              "block truncate",
                              last ? "text-foreground" : "text-muted-foreground"
                            )}
                            aria-current={last ? "page" : undefined}
                          >
                            {crumb.label}
                          </span>
                        )}
                      </li>
                    </Fragment>
                  )
                })}
              </ol>
            </nav>
          )}

          <div className="ml-auto flex shrink-0 items-center gap-1">
            {config.useMockApi && (
              <span className="mr-1 hidden items-center gap-1.5 rounded-full border border-border px-2 py-0.5 text-xs text-muted-foreground sm:inline-flex">
                <span
                  aria-hidden="true"
                  className="size-1.5 rounded-full bg-primary"
                />
                Mock data
              </span>
            )}

            <Button
              variant="ghost"
              size="icon-sm"
              onClick={toggleTheme}
              aria-label="Toggle colour theme"
            >
              <RiSunLine className="dark:hidden" />
              <RiMoonLine className="hidden dark:block" />
            </Button>

            <DropdownMenu>
              <DropdownMenuTrigger
                render={
                  <Button
                    variant="ghost"
                    size="icon"
                    aria-label="Account menu"
                  />
                }
              >
                <Avatar size="sm">
                  <AvatarFallback className="text-[0.6875rem]">
                    {initials(profile.displayName)}
                  </AvatarFallback>
                </Avatar>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-60">
                <div className="px-2 py-1.5">
                  <span className="block truncate text-sm font-medium">
                    {profile.displayName}
                  </span>
                  <span className="block truncate text-xs text-muted-foreground">
                    {profile.email}
                  </span>
                </div>
                <DropdownMenuSeparator />
                <DropdownMenuItem render={<Link to="/profile" />}>
                  <RiUser3Line />
                  Profile settings
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem variant="destructive" onClick={signOut}>
                  <RiLogoutBoxRLine />
                  Sign out
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </div>
      </header>

      <div className="lg:grid lg:grid-cols-[var(--sidebar-width)_minmax(0,1fr)]">
        <Sidebar
          sections={sections}
          backLink={backLink}
          scopeLabel={scopeLabel}
        />

        <main
          id="main"
          className="min-w-0 px-4 py-8 sm:px-6 sm:py-10 lg:px-8"
        >
          <div className="mx-auto w-full max-w-[68rem]">{children}</div>
        </main>
      </div>
    </div>
  );
}

export function PageHeader({
  title,
  description,
  actions,
}: {
  title: React.ReactNode
  description?: React.ReactNode
  actions?: React.ReactNode
}) {
  return (
    <div className="mb-8 flex flex-wrap items-start justify-between gap-x-6 gap-y-4">
      <div className="min-w-0 space-y-1.5">
        <h1 className="text-2xl leading-none font-medium tracking-tight">
          {title}
        </h1>
        {description && (
          <p className="max-w-[62ch] text-sm text-muted-foreground">
            {description}
          </p>
        )}
      </div>
      {actions && (
        <div className="flex shrink-0 items-center gap-2">{actions}</div>
      )}
    </div>
  );
}

export function StatusPill({
  tone,
  children,
}: {
  tone: "active" | "pending" | "error"
  children: React.ReactNode
}) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 text-xs whitespace-nowrap",
        tone === "active" && "text-success",
        tone === "pending" && "text-primary-ink",
        tone === "error" && "text-destructive"
      )}
    >
      <span
        aria-hidden="true"
        className={cn(
          "size-1.5 shrink-0 rounded-full bg-current",
          tone === "pending" && "animate-pulse"
        )}
      />
      {children}
    </span>
  );
}

export function EmptyState({
  icon,
  title,
  description,
  action,
}: {
  icon: React.ReactNode
  title: string
  description: React.ReactNode
  action?: React.ReactNode
}) {
  return (
    <div className="flex flex-col items-center gap-4 rounded-xl border border-dashed px-6 py-16 text-center">
      <div className="text-muted-foreground/70" aria-hidden="true">
        {icon}
      </div>
      <div className="space-y-1.5">
        <p className="font-medium">{title}</p>
        <p className="mx-auto max-w-[46ch] text-sm text-muted-foreground">
          {description}
        </p>
      </div>
      {action}
    </div>
  );
}
