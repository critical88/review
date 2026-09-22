import React from 'react';
import styled, { CSSObject } from '@emotion/styled';
import classnames from 'classnames';
import { useMediaQuery } from '../hooks/useMediaQuery';
import { sidebarClasses } from '../utils/utilityClasses';
import { StyledBackdrop } from '../styles/StyledBackdrop';

type PredefinedBreakPoint = 'xs' | 'sm' | 'md' | 'lg' | 'xl' | 'xxl' | 'all';

/**
 * Accepts a predefined breakpoint (with editor autocomplete) or any custom CSS
 * length such as `'450px'`. The `string & {}` keeps the literal suggestions
 * while still allowing arbitrary strings.
 */
type BreakPoint = PredefinedBreakPoint | (string & {});

const BREAK_POINTS: Record<Exclude<PredefinedBreakPoint, 'all'>, string> = {
  xs: '480px',
  sm: '576px',
  md: '768px',
  lg: '992px',
  xl: '1200px',
  xxl: '1600px',
};

export interface SidebarProps extends React.HTMLAttributes<HTMLHtmlElement> {
  /**
   * sidebar collapsed status
   */
  collapsed?: boolean;

  /**
   * width of the sidebar
   * @default ```250px```
   */
  width?: string;

  /**
   * width of the sidebar when collapsed
   * @default ```80px```
   */
  collapsedWidth?: string;

  /**
   * set when the sidebar should trigger responsiveness behavior.
   * accepts a predefined breakpoint (`xs | sm | md | lg | xl | xxl | all`) or a
   * custom CSS value such as `'450px'`
   * @type `xs | sm | md | lg | xl | xxl | all | (string & {}) | undefined`
   */
  breakPoint?: BreakPoint;

  /**
   * sidebar background color
   */
  backgroundColor?: string;

  /**
   * duration for the transition in milliseconds to be used in collapse and toggle behavior
   * @default ```300```
   */
  transitionDuration?: number;

  /**
   * sidebar background image
   */
  image?: string;

  /**
   * sidebar direction
   */
  rtl?: boolean;

  /**
   * sidebar toggled status
   */
  toggled?: boolean;

  /**
   * callback function to be called when backdrop is clicked
   */
  onBackdropClick?: () => void;

  /**
   * callback function to be called when sidebar's broken state changes
   */
  onBreakPoint?: (broken: boolean) => void;

  /**
   * sidebar styles to be applied from the root element
   */
  rootStyles?: CSSObject;

  children?: React.ReactNode;
}

interface StyledSidebarProps extends Omit<SidebarProps, 'backgroundColor'> {
  collapsed?: boolean;
  toggled?: boolean;
  broken?: boolean;
  rtl?: boolean;
  /** Media query (e.g. `(max-width: 768px)` / `screen`) at which to hide via CSS. */
  breakpointMediaQuery?: string;
}

type StyledSidebarContainerProps = Pick<SidebarProps, 'backgroundColor'>;

// Off-canvas (broken/overlay) layout: fixed and slid off-screen, brought back
// when toggled. Shared by the CSS media query and the runtime `broken` class so
// the two never drift.
const brokenLayout = ({
  rtl,
  width,
  collapsedWidth,
}: Pick<StyledSidebarProps, 'rtl' | 'width' | 'collapsedWidth'>) => `
  position: fixed;
  height: 100%;
  top: 0px;
  z-index: 100;
  ${rtl ? `right: -${width};` : `left: -${width};`}

  &.${sidebarClasses.collapsed} {
    ${rtl ? `right: -${collapsedWidth};` : `left: -${collapsedWidth};`}
  }

  &.${sidebarClasses.toggled} {
    ${rtl ? `right: 0;` : `left: 0;`}
  }
`;

const StyledSidebar = styled.aside<StyledSidebarProps>`
  position: relative;
  border-right-width: 1px;
  border-right-style: solid;
  border-color: #efefef;

  transition: ${({ transitionDuration }) => `width, left, right, ${transitionDuration}ms`};

  width: ${({ width }) => width};
  min-width: ${({ width }) => width};

  &.${sidebarClasses.collapsed} {
    width: ${({ collapsedWidth }) => collapsedWidth};
    min-width: ${({ collapsedWidth }) => collapsedWidth};
  }

  &.${sidebarClasses.rtl} {
    direction: rtl;
    border-right-width: none;
    border-left-width: 1px;
    border-right-style: none;
    border-left-style: solid;
  }

  /*
   * Hide below the breakpoint with a real CSS media query so the correct
   * (hidden) layout is painted on the very first load — before JS runs and
   * hydrates. Without this, SSR (e.g. Next.js) renders the sidebar visible
   * (the server can't know the viewport), causing a flash before the runtime
   * \`broken\` state hides it.
   */
  ${({ breakpointMediaQuery, ...props }) =>
    breakpointMediaQuery ? `@media ${breakpointMediaQuery} { ${brokenLayout(props)} }` : ''}

  /* Runtime broken state (covers resize after mount; also a styling hook). */
  &.${sidebarClasses.broken} {
    ${(props) => brokenLayout(props)}
  }

  ${({ rootStyles }) => rootStyles}
`;

const StyledSidebarContainer = styled.div<StyledSidebarContainerProps>`
  position: relative;
  height: 100%;
  overflow-y: auto;
  overflow-x: hidden;
  z-index: 3;

  ${({ backgroundColor }) => (backgroundColor ? `background-color:${backgroundColor};` : '')}
`;

const StyledSidebarImage = styled.img`
  &.${sidebarClasses.image} {
    width: 100%;
    height: 100%;
    object-fit: cover;
    object-position: center;
    position: absolute;
    left: 0;
    top: 0;
    z-index: 2;
  }
`;

interface SidebarContextProps {
  collapsed?: boolean;
  toggled?: boolean;
  rtl?: boolean;
  transitionDuration?: number;
}

export const SidebarContext = React.createContext<SidebarContextProps>({
  collapsed: false,
  toggled: false,
  rtl: false,
  transitionDuration: 300,
});

export const Sidebar = React.forwardRef<HTMLHtmlElement, SidebarProps>(
  (
    {
      collapsed,
      toggled,
      onBackdropClick,
      onBreakPoint,
      width = '250px',
      collapsedWidth = '80px',
      className,
      children,
      breakPoint,
      backgroundColor = 'rgb(249, 249, 249, 0.7)',
      transitionDuration = 300,
      image,
      rtl,
      rootStyles,
      ...rest
    },
    ref,
  ) => {
    const getBreakpointValue = () => {
      if (!breakPoint) return undefined;

      if (breakPoint === 'all') {
        return `screen`;
      }

      // predefined breakpoint -> its px value; otherwise treat as a custom value
      if (breakPoint in BREAK_POINTS) {
        return `(max-width: ${BREAK_POINTS[breakPoint as Exclude<PredefinedBreakPoint, 'all'>]})`;
      }

      return `(max-width: ${breakPoint})`;
    };

    const breakpointCallbackFnRef = React.useRef<(broken: boolean) => void>();

    breakpointCallbackFnRef.current = (broken: boolean) => {
      onBreakPoint?.(broken);
    };

    const breakpointMediaQuery = getBreakpointValue();
    const broken = useMediaQuery(breakpointMediaQuery);

    const collapsedValue = collapsed;
    const toggledValue = toggled;

    const handleBackdropClick = () => {
      onBackdropClick?.();
    };

    const handleBackdropKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
      if (event.key === 'Enter' || event.key === ' ') {
        event.preventDefault();
        onBackdropClick?.();
      }
    };

    const innerRef = React.useRef<HTMLHtmlElement | null>(null);

    const setSidebarRef = React.useCallback(
      (node: HTMLHtmlElement | null) => {
        innerRef.current = node;
        if (typeof ref === 'function') {
          ref(node);
        } else if (ref) {
          (ref as React.MutableRefObject<HTMLHtmlElement | null>).current = node;
        }
      },
      [ref],
    );

    const isOverlayOpen = !!(broken && toggledValue);

    const sidebarContextValue = React.useMemo(
      () => ({ collapsed: collapsedValue, toggled: toggledValue, rtl, transitionDuration }),
      [collapsedValue, toggledValue, rtl, transitionDuration],
    );

    // Notify `onBreakPoint` only when the broken state actually changes.
    // `broken` starts `false` (deterministic for SSR) and settles via a layout
    // effect, so a viewport that already matches would otherwise emit a
    // spurious `false` before the settled `true`. The `false` baseline means
    // the default (non-broken) state is never re-announced on mount.
    const prevBrokenRef = React.useRef(false);
    React.useEffect(() => {
      if (broken !== prevBrokenRef.current) {
        prevBrokenRef.current = broken;
        breakpointCallbackFnRef.current?.(broken);
      }
    }, [broken]);

    // When the sidebar opens as an overlay (broken + toggled), move focus to it
    // so keyboard users land inside the sidebar rather than on the backdrop.
    React.useEffect(() => {
      if (isOverlayOpen) {
        innerRef.current?.focus();
      }
    }, [isOverlayOpen]);

    // Close the overlay sidebar on Escape from anywhere.
    React.useEffect(() => {
      if (!isOverlayOpen) return undefined;

      const handleEscape = (event: KeyboardEvent) => {
        if (event.key === 'Escape') {
          onBackdropClick?.();
        }
      };

      document.addEventListener('keydown', handleEscape);
      return () => document.removeEventListener('keydown', handleEscape);
    }, [isOverlayOpen, onBackdropClick]);

    return (
      <SidebarContext.Provider value={sidebarContextValue}>
        <StyledSidebar
          ref={setSidebarRef}
          data-testid={`${sidebarClasses.root}-test-id`}
          tabIndex={-1}
          rtl={rtl}
          rootStyles={rootStyles}
          width={width}
          collapsedWidth={collapsedWidth}
          transitionDuration={transitionDuration}
          breakpointMediaQuery={breakpointMediaQuery}
          className={classnames(
            sidebarClasses.root,
            {
              [sidebarClasses.collapsed]: collapsedValue,
              [sidebarClasses.toggled]: toggledValue,
              [sidebarClasses.broken]: broken,
              [sidebarClasses.rtl]: rtl,
            },
            className,
          )}
          {...rest}
        >
          <StyledSidebarContainer
            data-testid={`${sidebarClasses.container}-test-id`}
            className={sidebarClasses.container}
            backgroundColor={backgroundColor}
          >
            {children}
          </StyledSidebarContainer>

          {image && (
            <StyledSidebarImage
              data-testid={`${sidebarClasses.image}-test-id`}
              src={image}
              alt="sidebar background"
              className={sidebarClasses.image}
            />
          )}

          {broken && toggledValue && (
            <StyledBackdrop
              data-testid={`${sidebarClasses.backdrop}-test-id`}
              role="button"
              tabIndex={0}
              aria-label="backdrop"
              onClick={handleBackdropClick}
              onKeyDown={handleBackdropKeyDown}
              className={sidebarClasses.backdrop}
            />
          )}
        </StyledSidebar>
      </SidebarContext.Provider>
    );
  },
);

Sidebar.displayName = 'Sidebar';
