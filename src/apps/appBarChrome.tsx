import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
  type ReactNode,
} from "react";

export type MainPanelAppBarChrome = {
  /** Replaces the default app title next to the back button. */
  leading?: ReactNode;
  /** When true, leading expands to fill remaining header space. */
  leadingGrow?: boolean;
  /** Rendered before the app icon on the right. */
  trailing?: ReactNode;
  /** Hide the decorative app icon on the right of the app bar. */
  hideIcon?: boolean;
};

type MainPanelAppBarChromeContextValue = {
  chrome: MainPanelAppBarChrome;
  setChrome: (chrome: MainPanelAppBarChrome) => void;
};

const MainPanelAppBarChromeContext =
  createContext<MainPanelAppBarChromeContextValue | null>(null);

export function MainPanelAppBarChromeProvider({
  children,
}: {
  children: ReactNode;
}) {
  const [chrome, setChromeState] = useState<MainPanelAppBarChrome>({});
  const setChrome = useCallback((next: MainPanelAppBarChrome) => {
    setChromeState(next);
  }, []);
  const value = useMemo(
    () => ({ chrome, setChrome }),
    [chrome, setChrome],
  );

  return (
    <MainPanelAppBarChromeContext.Provider value={value}>
      {children}
    </MainPanelAppBarChromeContext.Provider>
  );
}

export function useMainPanelAppBarChrome() {
  const value = useContext(MainPanelAppBarChromeContext);
  if (!value) {
    throw new Error(
      "useMainPanelAppBarChrome must be used within MainPanelAppBarChromeProvider",
    );
  }
  return value;
}

export function useOptionalMainPanelAppBarChrome() {
  return useContext(MainPanelAppBarChromeContext);
}
