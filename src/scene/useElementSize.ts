import { useLayoutEffect, useState } from "react";
import type { RefObject } from "react";

export interface Size {
  readonly width: number;
  readonly height: number;
}

/** Tracks an element's content-box size. */
export function useElementSize(ref: RefObject<HTMLElement | null>): Size {
  const [size, setSize] = useState<Size>({ width: 0, height: 0 });
  useLayoutEffect(() => {
    const el = ref.current;
    if (el === null) return;
    const measure = (): void => {
      setSize({ width: el.clientWidth, height: el.clientHeight });
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => {
      observer.disconnect();
    };
  }, [ref]);
  return size;
}
