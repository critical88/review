import type { ToastT } from './types';

// How a toast resolves the visual skin it renders with. Values can come from
// the toast itself or from the toaster rendering it, and the pieces below spell
// out which one wins for every part of the skin.

// A toast renders inverted when the toast itself or the toaster it belongs to
// asks for it. `invert` is tri-state: unset on the toast means "do what the
// toaster does".
export const getToastInvert = (toast: ToastT, toasterInvert: boolean | undefined) => {
  return toast.invert || toasterInvert;
};

// Rich colors are the colored variant of a type: green success, red error and so
// on. The toast opts in itself or, when it doesn't care, the toaster decides.
export const getToastRichColors = (toast: ToastT, defaultRichColors: boolean | undefined) => {
  return toast.richColors ?? defaultRichColors;
};

// Only toasts that don't bring their own JSX and are not opted out of the
// built-in styling are rendered with it. Everything else is on its own.
export const getToastStyled = (toast: ToastT, toasterUnstyled: boolean | undefined) => {
  return !Boolean(toast.jsx || toast.unstyled || toasterUnstyled);
};
