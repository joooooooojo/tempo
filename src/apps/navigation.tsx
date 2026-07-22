import { createContext, useContext, type ReactNode } from "react";
import type { OpenBuiltinAppOptions } from "@/apps/types";

type BuiltinAppNavigationValue = {
  openApp: (appId: string, options?: OpenBuiltinAppOptions) => void;
  backToSearch: () => void;
};

const BuiltinAppNavigationContext = createContext<BuiltinAppNavigationValue | null>(null);

export function BuiltinAppNavigationProvider({
  value,
  children,
}: {
  value: BuiltinAppNavigationValue;
  children: ReactNode;
}) {
  return (
    <BuiltinAppNavigationContext.Provider value={value}>
      {children}
    </BuiltinAppNavigationContext.Provider>
  );
}

export function useBuiltinAppNavigation() {
  const value = useContext(BuiltinAppNavigationContext);
  if (!value) {
    throw new Error("useBuiltinAppNavigation must be used within BuiltinAppNavigationProvider");
  }
  return value;
}

/** Safe optional access when a page may also render outside the palette shell. */
export function useOptionalBuiltinAppNavigation() {
  return useContext(BuiltinAppNavigationContext);
}
