import { useCallback, useEffect, useRef, useState } from "react";
import type { AppIntent, AppViewState } from "../state/controller";
import { DesktopProductEffectPort } from "./DesktopProductEffectPort";
import { ProductRuntime } from "./ProductRuntime";

/** React composition hook; the Runtime remains the sole product-state owner. */
export function useDesktopProduct(
  configuration: "configured" | "not_configured" | undefined,
  enabled = true,
): { readonly view?: AppViewState; readonly dispatch: (intent: AppIntent) => void } {
  const runtime = useRef<ProductRuntime | undefined>(undefined);
  const [view, setView] = useState<AppViewState>();

  useEffect(() => {
    if (!enabled || configuration === undefined) return;
    const product = new ProductRuntime(new DesktopProductEffectPort(), configuration);
    runtime.current = product;
    const unsubscribe = product.subscribe(setView);
    product.dispatch({ type: "boot" });
    return () => { unsubscribe(); product.dispose(); runtime.current = undefined; };
  }, [configuration, enabled]);

  const dispatch = useCallback((intent: AppIntent) => runtime.current?.dispatch(intent), []);
  return { view, dispatch };
}
