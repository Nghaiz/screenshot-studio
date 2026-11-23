// `motion/react` transitions scaled by the theme's motion personality x the intensity setting
// (design-intent § Look and motion). Factor 0 (off, or prefers-reduced-motion) makes every transition instant.
import { useMemo } from "react";
import type { Transition } from "motion/react";
import { usePrefs } from "../state/prefs";
import { effectiveMotion, MOTION_INTENSITY, motionFactor } from "../themes";

export interface MotionPreset {
  factor: number;
  fade: Transition;
  pop: Transition;
}

export function useMotion(): MotionPreset {
  const theme = usePrefs((s) => s.theme);
  const level = usePrefs((s) => s.motion);
  return useMemo(() => {
    // motionFactor reads the applied CSS; recompute when the theme or level changes.
    void theme;
    const factor = MOTION_INTENSITY[effectiveMotion(level)] === 0 ? 0 : motionFactor();
    if (factor === 0) return { factor, fade: { duration: 0 }, pop: { duration: 0 } };
    return {
      factor,
      fade: { duration: 0.18 * factor },
      pop: { type: "spring", stiffness: 420 / factor, damping: 32, mass: 0.9 * factor },
    };
  }, [theme, level]);
}
