import React from 'react';
import classnames from 'classnames';
import { StyledUl } from '../styles/StyledUl';
import styled, { CSSObject } from '@emotion/styled';
import { menuClasses } from '../utils/utilityClasses';

export interface MenuItemStylesParams {
  level: number;
  disabled: boolean;
  active: boolean;
  isSubmenu: boolean;
  open?: boolean;
}

export type ElementStyles = CSSObject | ((params: MenuItemStylesParams) => CSSObject | undefined);

export interface MenuItemStyles {
  root?: ElementStyles;
  button?: ElementStyles;
  label?: ElementStyles;
  prefix?: ElementStyles;
  suffix?: ElementStyles;
  icon?: ElementStyles;
  subMenuContent?: ElementStyles;
  SubMenuExpandIcon?: ElementStyles;
}

/**
 * Resolve the styles for a single element of `menuItemStyles`. The value is
 * either a style object or a function of the item's state.
 */
export const resolveElementStyles = (
  styles: MenuItemStyles | undefined,
  element: keyof MenuItemStyles,
  params: MenuItemStylesParams,
): CSSObject | undefined => {
  const style = styles?.[element];
  return typeof style === 'function' ? style(params) : style;
};

export interface RenderExpandIconParams {
  level: number;
  disabled: boolean;
  active: boolean;
  open: boolean;
}

export interface MenuContextProps {
  /**
   * Transition duration in milliseconds
   * @default ```300```
   */
  transitionDuration?: number;

  /**
   * If set to true, the popper menu will close when a menu item is clicked
   * This works on collapsed mode only
   * @default ```false```
   */
  closeOnClick?: boolean;

  /**
   * If set to true, top-level `SubMenu`s open as floating poppers even when the
   * sidebar is expanded (instead of sliding open inline). Useful for tall
   * sidebars with many items.
   * @default ```false```
   */
  popover?: boolean;

  /**
   * Apply styles to MenuItem and SubMenu components and their children
   */
  menuItemStyles?: MenuItemStyles;

  /**
   * Render a custom expand icon for SubMenu components
   */
  renderExpandIcon?: (params: RenderExpandIconParams) => React.ReactNode;
}

export interface MenuProps extends MenuContextProps, React.MenuHTMLAttributes<HTMLMenuElement> {
  /**
   * If `true`, only one top-level `SubMenu` can be open at a time (accordion
   * behavior). Opening another closes the previously open one.
   * @default ```false```
   */
  accordion?: boolean;

  rootStyles?: CSSObject;
  children?: React.ReactNode;
}

/**
 * Internal context used to coordinate single-open (accordion) behavior across
 * sibling `SubMenu`s at a given level. A `null` value means no accordion is
 * active at this level.
 */
export type AccordionContextValue = {
  activeId: string | null;
  setActive: (id: string, isOpen: boolean) => void;
  /**
   * Synchronously-mutable ref used during mount to detect when more than one
   * `SubMenu` registers `defaultOpen` in the same accordion group (sibling
   * effects in the same commit otherwise can't see each other's state updates).
   */
  defaultOpenRegisteredRef: React.MutableRefObject<string | null>;
} | null;

export const AccordionContext = React.createContext<AccordionContextValue>(null);

/**
 * Internal context used to cascade the `active` state up the tree: a child
 * (MenuItem or SubMenu) reports its active state to the nearest parent SubMenu,
 * which marks itself active when any descendant is active. `null` at the top
 * level (no parent SubMenu to report to).
 */
export type SubMenuActiveContextValue = {
  registerActive: (id: string, active: boolean) => void;
} | null;

export const SubMenuActiveContext = React.createContext<SubMenuActiveContextValue>(null);

const StyledMenu = styled.nav<Pick<MenuProps, 'rootStyles'>>`
  &.${menuClasses.root} {
    ${({ rootStyles }) => rootStyles}
  }
`;

export const MenuContext = React.createContext<MenuContextProps | undefined>(undefined);

export const LevelContext = React.createContext<number>(0);

const MenuFR: React.ForwardRefRenderFunction<HTMLMenuElement, MenuProps> = (
  {
    children,
    className,
    transitionDuration = 300,
    closeOnClick = false,
    accordion = false,
    popover = false,
    rootStyles,
    menuItemStyles,
    renderExpandIcon,
    ...rest
  },
  ref,
) => {
  const providerValue = React.useMemo(
    () => ({ transitionDuration, closeOnClick, popover, menuItemStyles, renderExpandIcon }),
    [transitionDuration, closeOnClick, popover, menuItemStyles, renderExpandIcon],
  );

  const [activeId, setActiveId] = React.useState<string | null>(null);
  const setActive = React.useCallback((id: string, isOpen: boolean) => {
    setActiveId((prev) => (isOpen ? id : prev === id ? null : prev));
  }, []);
  const defaultOpenRegisteredRef = React.useRef<string | null>(null);

  const accordionContext = React.useMemo<AccordionContextValue>(
    () => (accordion ? { activeId, setActive, defaultOpenRegisteredRef } : null),
    [accordion, activeId, setActive],
  );

  return (
    <MenuContext.Provider value={providerValue}>
      <LevelContext.Provider value={0}>
        <AccordionContext.Provider value={accordionContext}>
          <StyledMenu
            ref={ref}
            className={classnames(menuClasses.root, className)}
            rootStyles={rootStyles}
            {...rest}
          >
            <StyledUl>{children}</StyledUl>
          </StyledMenu>
        </AccordionContext.Provider>
      </LevelContext.Provider>
    </MenuContext.Provider>
  );
};

export const Menu = React.forwardRef<HTMLMenuElement, MenuProps>(MenuFR);
