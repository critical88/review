import React from 'react';
import { createPopper } from '@popperjs/core';
import { SidebarContext } from '../components/Sidebar';

interface PopperOptions {
  /** Whether this submenu should render as a floating popper. */
  popper: boolean;
  /**
   * Whether the content is in its final DOM location (it is portaled to
   * `<body>` after mount). Creating the popper before this would bind it to the
   * pre-portal node, which is then remounted — leaving the visible node
   * unpositioned.
   */
  mounted: boolean;
  buttonRef: React.RefObject<HTMLAnchorElement>;
  contentRef: React.RefObject<HTMLDivElement>;
}

interface PopperResult {
  popperInstance?: ReturnType<typeof createPopper>;
}

export const usePopper = (options: PopperOptions): PopperResult => {
  const { popper, mounted, buttonRef, contentRef } = options;

  const { toggled, transitionDuration } = React.useContext(SidebarContext);

  // Keep the instance in state (not just a ref) so consumers re-render and can
  // reliably call `.update()` once the popper exists.
  const [popperInstance, setPopperInstance] = React.useState<
    ReturnType<typeof createPopper> | undefined
  >();

  /**
   * Create the popper instance only when the submenu renders as a popper
   * (collapsed top-level submenu, or `popover` mode while expanded).
   */
  React.useEffect(() => {
    if (popper && mounted && contentRef.current && buttonRef.current) {
      const instance = createPopper(buttonRef.current, contentRef.current, {
        placement: 'right',
        strategy: 'fixed',
        modifiers: [
          {
            name: 'offset',
            options: {
              offset: [0, 5],
            },
          },
        ],
      });
      setPopperInstance(instance);

      return () => {
        instance.destroy();
        setPopperInstance(undefined);
      };
    }

    return undefined;
  }, [popper, mounted, contentRef, buttonRef]);

  /**
   * Keep the popper positioned: update when the trigger/content resize and once
   * the sidebar's collapse/toggle transition settles.
   */
  React.useEffect(() => {
    if (!popperInstance) return undefined;

    let resizeObserver: ResizeObserver | undefined;

    if (contentRef.current && buttonRef.current) {
      resizeObserver = new ResizeObserver(() => {
        popperInstance.update();
      });

      resizeObserver.observe(contentRef.current);
      resizeObserver.observe(buttonRef.current);
    }

    const timer = setTimeout(() => {
      popperInstance.update();
    }, transitionDuration);

    return () => {
      resizeObserver?.disconnect();
      clearTimeout(timer);
    };
  }, [popperInstance, transitionDuration, toggled, contentRef, buttonRef]);

  return { popperInstance };
};
