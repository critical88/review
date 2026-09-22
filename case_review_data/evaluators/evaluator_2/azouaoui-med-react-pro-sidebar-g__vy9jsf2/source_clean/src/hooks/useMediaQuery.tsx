import React from 'react';

// useLayoutEffect on the client so the correct value is committed before the
// browser paints (no flash of the wrong layout); fall back to useEffect on the
// server to avoid React's "useLayoutEffect does nothing on the server" warning.
const useIsomorphicLayoutEffect =
  typeof window !== 'undefined' ? React.useLayoutEffect : React.useEffect;

export const useMediaQuery = (breakpoint?: string): boolean => {
  // Always start `false` so the server render and the first client render
  // agree. Reading `window.matchMedia` in the initializer would make the
  // hydration render disagree with the server; React would keep the server
  // markup and the later sync (setting the same value) would be a no-op,
  // leaving the wrong layout committed — exactly the SSR bug this avoids.
  const [matches, setMatches] = React.useState(false);

  useIsomorphicLayoutEffect(() => {
    if (!breakpoint || typeof window === 'undefined') return undefined;

    const media = window.matchMedia(breakpoint);

    // setMatches with the current value; React bails out if it's unchanged.
    const handleMatch = () => setMatches(media.matches);

    // Sync the real value once mounted (and whenever the query changes).
    handleMatch();

    media.addEventListener('change', handleMatch);
    return () => media.removeEventListener('change', handleMatch);
  }, [breakpoint]);

  return matches;
};
