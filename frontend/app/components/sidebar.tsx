import { useEffect, useState } from "react";
import { NavLink, useLocation, useParams } from "react-router";
import {
  RiArrowLeftLine,
  RiFileList2Line,
  RiGlobalLine,
  RiServerLine,
  RiShieldKeyholeLine,
  RiSideBarLine,
  RiStackLine,
  RiUser3Line,
} from "@remixicon/react";

import { Button } from "~/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "~/components/ui/tooltip";
import { cn } from "~/lib/utils";

export interface NavItem {
  to: string;
  label: string;
  icon: React.ReactNode;
  end?: boolean;
}

export interface NavSection {
  label?: string;
  items: NavItem[];
}

export function accountNav(): NavSection[] {
  return [
    {
      items: [
        { to: "/", label: "Zones", icon: <RiGlobalLine />, end: true },
        { to: "/profile", label: "Profile", icon: <RiUser3Line /> },
      ],
    },
  ];
}

export function profileNav(): NavSection[] {
  return [
    {
      items: [
        { to: "/profile", label: "Details", icon: <RiUser3Line />, end: true },
        {
          to: "/profile/security",
          label: "Security",
          icon: <RiShieldKeyholeLine />,
        },
      ],
    },
  ];
}

export function zoneNav(zoneId: string): NavSection[] {
  const base = `/zones/${zoneId}`;

  return [
    {
      items: [
        { to: base, label: "Records", icon: <RiFileList2Line />, end: true },
        {
          to: `${base}/soa`,
          label: "Start of authority",
          icon: <RiStackLine />,
        },
        {
          to: `${base}/nameservers`,
          label: "Nameservers",
          icon: <RiServerLine />,
        },
      ],
    },
  ];
}

function useCollapsed(): [boolean, () => void] {
  const [collapsed, setCollapsed] = useState(false);

  useEffect(() => {
    setCollapsed(document.documentElement.getAttribute("data-sidebar") === "collapsed");
  }, []);

  const toggle = () => {
    const root = document.documentElement;
    const next = root.getAttribute("data-sidebar") !== "collapsed";

    if (next) {
      root.setAttribute("data-sidebar", "collapsed");
    } else {
      root.removeAttribute("data-sidebar");
    }

    try {
      localStorage.setItem("hermes-sidebar", next ? "collapsed" : "expanded");
    } catch {}

    setCollapsed(next);
  }

  return [ collapsed, toggle ];
}

function NavRow({
  item,
  collapsed,
  onNavigate,
}: {
  item: NavItem
  collapsed: boolean
  onNavigate?: () => void
}) {
  const row = (
    <NavLink
      to={item.to}
      end={item.end}
      onClick={onNavigate}
      className={({ isActive }) =>
        cn(
          "group/nav relative flex min-h-9 items-center gap-2.5 rounded-md px-2.5 text-sm transition-colors",
          "in-data-[sidebar=collapsed]:justify-center in-data-[sidebar=collapsed]:px-0",
          isActive
            ? "bg-surface-2 font-medium text-foreground"
            : "text-muted-foreground hover:bg-surface-2/70 hover:text-foreground"
        )
      }
    >
      {({ isActive }) => (
        <>
          <span
            aria-hidden="true"
            className={cn(
              "absolute top-1/2 left-0 h-4 w-0.5 -translate-y-1/2 rounded-r-full bg-primary transition-opacity",
              isActive ? "opacity-100" : "opacity-0"
            )}
          />
          <span className="grid size-4 shrink-0 place-items-center [&_svg]:size-4">
            {item.icon}
          </span>
          <span className="truncate in-data-[sidebar=collapsed]:hidden">
            {item.label}
          </span>
        </>
      )}
    </NavLink>
  );

  if (!collapsed) return row;

  return (
    <Tooltip>
      <TooltipTrigger render={<span className="block" />}>{row}</TooltipTrigger>
      <TooltipContent side="right">{item.label}</TooltipContent>
    </Tooltip>
  );
}

export function SidebarNav({
  sections,
  backLink,
  collapsed,
  onNavigate,
}: {
  sections: NavSection[]
  backLink?: { to: string; label: string }
  collapsed: boolean
  onNavigate?: () => void
}) {
  return (
    <nav aria-label="Main" className="flex flex-col gap-1 p-2">
      {backLink && (
        <>
          <NavRow
            item={{
              to: backLink.to,
              label: backLink.label,
              icon: <RiArrowLeftLine />,
              end: true,
            }}
            collapsed={collapsed}
            onNavigate={onNavigate}
          />
          <hr className="my-1.5 border-t in-data-[sidebar=collapsed]:mx-2" />
        </>
      )}

      {sections.map((section, index) => (
        <div key={section.label ?? index} className="flex flex-col gap-0.5">
          {section.label && (
            <p className="px-2.5 pt-2 pb-1 text-xs text-muted-foreground in-data-[sidebar=collapsed]:hidden">
              {section.label}
            </p>
          )}
          {section.items.map((item) => (
            <NavRow
              key={item.to}
              item={item}
              collapsed={collapsed}
              onNavigate={onNavigate}
            />
          ))}
        </div>
      ))}
    </nav>
  );
}

export function useSidebarSections(zoneApex?: string): {
  sections: NavSection[]
  backLink?: { to: string; label: string }
} {
  const { zoneId } = useParams();
  const { pathname } = useLocation();

  if (zoneId) {
    return {
      sections: zoneNav(zoneId),
      backLink: { to: "/", label: "All zones" },
    };
  }

  if (pathname.startsWith("/profile")) {
    return {
      sections: profileNav(),
      backLink: { to: "/", label: "All zones" },
    };
  }

  return { sections: accountNav() };
}

export function Sidebar({
  sections,
  backLink,
  scopeLabel,
}: {
  sections: NavSection[]
  backLink?: { to: string; label: string }
  scopeLabel?: string
}) {
  const [collapsed, toggle] = useCollapsed();

  return (
    <aside
      className="sticky top-[var(--header-height)] hidden h-[calc(100svh-var(--header-height))] flex-col border-r bg-surface lg:flex"
      style={{ width: "var(--sidebar-width)" }}
    >
      {scopeLabel && (
        <p
          className="font-data truncate px-4 pt-4 pb-1 text-xs text-muted-foreground in-data-[sidebar=collapsed]:hidden"
          title={scopeLabel}
        >
          {scopeLabel}
        </p>
      )}

      <div className="flex-1 overflow-y-auto">
        <SidebarNav
          sections={sections}
          backLink={backLink}
          collapsed={collapsed}
        />
      </div>

      <div className="border-t p-2">
        <Button
          variant="ghost"
          size="sm"
          onClick={toggle}
          aria-expanded={!collapsed}
          aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          className="w-full justify-start gap-2.5 px-2.5 text-muted-foreground in-data-[sidebar=collapsed]:justify-center in-data-[sidebar=collapsed]:px-0"
        >
          <RiSideBarLine className="shrink-0" />
          <span className="in-data-[sidebar=collapsed]:hidden">Collapse</span>
        </Button>
      </div>
    </aside>
  );
}
