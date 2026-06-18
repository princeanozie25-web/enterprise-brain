"use client";

import { motion, useReducedMotion, type HTMLMotionProps, type Transition } from "framer-motion";
import { MOTION } from "@/lib/tokens";

type MotionChromeProps = {
  delayIndex?: number;
};

function motionTransition(shouldReduce: boolean, delayIndex = 0): Transition {
  return {
    delay: shouldReduce ? 0 : delayIndex * MOTION.framerStagger,
    duration: shouldReduce ? 0 : MOTION.framerView,
    ease: [...MOTION.framerEase],
  };
}

function hoverState(shouldReduce: boolean) {
  return { y: shouldReduce ? 0 : MOTION.framerHoverY };
}

function tapState(shouldReduce: boolean) {
  return { scale: shouldReduce ? 1 : MOTION.framerTapScale };
}

function initialState() {
  return { opacity: 0, y: MOTION.framerEnterY };
}

export function MotionPanel({
  delayIndex = 0,
  ...props
}: HTMLMotionProps<"div"> & MotionChromeProps) {
  const shouldReduce = useReducedMotion() ?? false;
  return (
    <motion.div
      initial={initialState()}
      animate={{ opacity: 1, y: 0 }}
      transition={motionTransition(shouldReduce, delayIndex)}
      {...props}
    />
  );
}

export function MotionCard({
  delayIndex = 0,
  ...props
}: HTMLMotionProps<"div"> & MotionChromeProps) {
  const shouldReduce = useReducedMotion() ?? false;
  return (
    <motion.div
      initial={initialState()}
      animate={{ opacity: 1, y: 0 }}
      whileHover={hoverState(shouldReduce)}
      whileTap={tapState(shouldReduce)}
      transition={motionTransition(shouldReduce, delayIndex)}
      {...props}
    />
  );
}

export function MotionAnchor({
  delayIndex = 0,
  ...props
}: HTMLMotionProps<"a"> & MotionChromeProps) {
  const shouldReduce = useReducedMotion() ?? false;
  return (
    <motion.a
      initial={initialState()}
      animate={{ opacity: 1, y: 0 }}
      whileHover={hoverState(shouldReduce)}
      whileTap={tapState(shouldReduce)}
      transition={motionTransition(shouldReduce, delayIndex)}
      {...props}
    />
  );
}

export function MotionArticle({
  delayIndex = 0,
  ...props
}: HTMLMotionProps<"article"> & MotionChromeProps) {
  const shouldReduce = useReducedMotion() ?? false;
  return (
    <motion.article
      initial={initialState()}
      animate={{ opacity: 1, y: 0 }}
      transition={motionTransition(shouldReduce, delayIndex)}
      {...props}
    />
  );
}

export function MotionSection({
  delayIndex = 0,
  ...props
}: HTMLMotionProps<"section"> & MotionChromeProps) {
  const shouldReduce = useReducedMotion() ?? false;
  return (
    <motion.section
      initial={initialState()}
      animate={{ opacity: 1, y: 0 }}
      transition={motionTransition(shouldReduce, delayIndex)}
      {...props}
    />
  );
}

export function MotionAside({
  delayIndex = 0,
  ...props
}: HTMLMotionProps<"aside"> & MotionChromeProps) {
  const shouldReduce = useReducedMotion() ?? false;
  return (
    <motion.aside
      initial={initialState()}
      animate={{ opacity: 1, y: 0 }}
      transition={motionTransition(shouldReduce, delayIndex)}
      {...props}
    />
  );
}
