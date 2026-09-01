export interface ScrollGeometry {
  readonly scrollTop: number;
  readonly clientHeight: number;
  readonly scrollHeight: number;
}

/** Visible overflow edges for progressive desktop scroll-surface treatment. */
export function visibleScrollEdges(geometry: ScrollGeometry) {
  return {
    top: geometry.scrollTop > 1,
    bottom: geometry.scrollTop + geometry.clientHeight < geometry.scrollHeight - 1,
  } as const;
}
