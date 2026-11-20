// Whether the person asked their computer for less motion: animations that loop or move things around are skipped
export const reducedMotion = () => typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
