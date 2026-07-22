import { createContext, useContext, type ReactNode } from "react";
import type { OpenAppOptions } from "@/apps/types";

type AppNavigationValue = {
  openApp: (appId: string, options?: OpenAppOptions) => void;
  backToSearch: () => void;
};

const AppNavigationContext = createContext<AppNavigationValue | null>(null);

export function AppNavigationProvider({
  value,
  children,
}: {
  value: AppNavigationValue;
  children: ReactNode;
}) {
  return <AppNavigationContext.Provider value={value}>{children}</AppNavigationContext.Provider>;
}

/** @deprecated Prefer AppNavigationProvider */
export const BuiltinAppNavigationProvider = AppNavigationProvider;

export function useAppNavigation() {
  const value = useContext(AppNavigationContext);
  if (!value) {
    throw new Error("useAppNavigation must be used within AppNavigationProvider");
  }
  return value;
}

/** @deprecated Prefer useAppNavigation */
export function useBuiltinAppNavigation() {
  return useAppNavigation();
}

/** Safe optional access when a page may also render outside the palette shell. */
export function useOptionalAppNavigation() {
  return useContext(AppNavigationContext);
}

/** @deprecated Prefer useOptionalAppNavigation */
export function useOptionalBuiltinAppNavigation() {
  return useOptionalAppNavigation();
}
