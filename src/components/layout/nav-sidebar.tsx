import { createContext, useContext, type ReactNode } from "react";

type NavSidebarContextValue = {
  /** Icon-rail vs full labels. Ignored when hidden. */
  collapsed: boolean;
  setCollapsed: (collapsed: boolean) => void;
  toggleCollapsed: () => void;
  /** Tool detail pages: sidebar fully removed. */
  hidden: boolean;
};

const NavSidebarContext = createContext<NavSidebarContextValue | null>(null);

export function NavSidebarProvider({
  value,
  children,
}: {
  value: NavSidebarContextValue;
  children: ReactNode;
}) {
  return <NavSidebarContext.Provider value={value}>{children}</NavSidebarContext.Provider>;
}

export function useNavSidebar() {
  const ctx = useContext(NavSidebarContext);
  if (!ctx) {
    throw new Error("useNavSidebar must be used within NavSidebarProvider");
  }
  return ctx;
}

export function useNavSidebarOptional() {
  return useContext(NavSidebarContext);
}
