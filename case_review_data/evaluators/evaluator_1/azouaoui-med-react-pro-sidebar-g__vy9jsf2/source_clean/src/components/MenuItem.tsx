import React from 'react';
import styled, { CSSObject } from '@emotion/styled';
import classnames from 'classnames';
import { StyledMenuLabel } from '../styles/StyledMenuLabel';
import { StyledMenuIcon } from '../styles/StyledMenuIcon';
import { StyledMenuPrefix } from '../styles/StyledMenuPrefix';
import { useMenu } from '../hooks/useMenu';
import { StyledMenuSuffix } from '../styles/StyledMenuSuffix';
import { menuClasses } from '../utils/utilityClasses';
import { MenuButton, menuButtonStyles } from './MenuButton';
import { LevelContext, resolveElementStyles, SubMenuActiveContext } from './Menu';
import { SidebarContext } from './Sidebar';

export interface MenuItemProps
  extends Omit<React.AnchorHTMLAttributes<HTMLAnchorElement>, 'prefix'> {
  /**
   * The icon to be displayed in the menu item
   */
  icon?: React.ReactNode;

  /**
   * The prefix to be displayed in the menu item
   */
  prefix?: React.ReactNode;

  /**
   * The suffix to be displayed in the menu item
   */
  suffix?: React.ReactNode;

  /**
   * If set to true, the menu item will have an active state
   * @default ```false```
   */
  active?: boolean;

  /**
   * If set to true, the menu item will be disabled
   * @default ```false```
   */
  disabled?: boolean;

  /**
   * The component to be rendered as the menu item button
   */
  component?: string | React.ReactElement;

  /**
   * Apply styles from the root element
   */
  rootStyles?: CSSObject;

  children?: React.ReactNode;
}

interface StyledMenuItemProps extends Pick<MenuItemProps, 'rootStyles' | 'active' | 'disabled'> {
  level: number;
  menuItemStyles?: CSSObject;
  collapsed?: boolean;
  rtl?: boolean;
  buttonStyles?: CSSObject;
}

type MenuItemElement = 'root' | 'button' | 'label' | 'prefix' | 'suffix' | 'icon';

const StyledMenuItem = styled.li<StyledMenuItemProps>`
  width: 100%;
  position: relative;

  ${({ menuItemStyles }) => menuItemStyles};

  ${({ rootStyles }) => rootStyles};

  > .${menuClasses.button} {
    ${({ level, disabled, active, collapsed, rtl }) =>
      menuButtonStyles({
        level,
        disabled,
        active,
        collapsed,
        rtl,
      })};

    ${({ buttonStyles }) => buttonStyles};
  }
`;

export const MenuItemFR: React.ForwardRefRenderFunction<HTMLLIElement, MenuItemProps> = (
  {
    children,
    icon,
    className,
    prefix,
    suffix,
    active = false,
    disabled = false,
    component,
    rootStyles,
    ...rest
  },
  ref,
) => {
  const level = React.useContext(LevelContext);
  const { collapsed, rtl, transitionDuration } = React.useContext(SidebarContext);
  const { menuItemStyles } = useMenu();

  // Report active state up to the nearest parent SubMenu so ancestors can
  // highlight when this item is active.
  const id = React.useId();
  const parentActive = React.useContext(SubMenuActiveContext);
  React.useEffect(() => {
    parentActive?.registerActive(id, active);
    return () => parentActive?.registerActive(id, false);
  }, [parentActive, id, active]);

  const styleParams = { level, disabled, active, isSubmenu: false };

  const getMenuItemStyles = (element: MenuItemElement): CSSObject | undefined =>
    resolveElementStyles(menuItemStyles, element, styleParams);

  const sharedClasses = {
    [menuClasses.active]: active,
    [menuClasses.disabled]: disabled,
  };

  return (
    <StyledMenuItem
      ref={ref}
      className={classnames(menuClasses.menuItemRoot, sharedClasses, className)}
      menuItemStyles={getMenuItemStyles('root')}
      level={level}
      collapsed={collapsed}
      rtl={rtl}
      disabled={disabled}
      active={active}
      buttonStyles={getMenuItemStyles('button')}
      rootStyles={rootStyles}
    >
      <MenuButton
        className={classnames(menuClasses.button, sharedClasses)}
        data-testid={`${menuClasses.button}-test-id`}
        component={component}
        tabIndex={0}
        aria-current={active ? 'page' : undefined}
        aria-disabled={disabled || undefined}
        {...rest}
      >
        {icon && (
          <StyledMenuIcon
            rtl={rtl}
            className={classnames(menuClasses.icon, sharedClasses)}
            rootStyles={getMenuItemStyles('icon')}
          >
            {icon}
          </StyledMenuIcon>
        )}

        {prefix && (
          <StyledMenuPrefix
            collapsed={collapsed}
            transitionDuration={transitionDuration}
            firstLevel={level === 0}
            className={classnames(menuClasses.prefix, sharedClasses)}
            rtl={rtl}
            rootStyles={getMenuItemStyles('prefix')}
          >
            {prefix}
          </StyledMenuPrefix>
        )}

        <StyledMenuLabel
          className={classnames(menuClasses.label, sharedClasses)}
          rootStyles={getMenuItemStyles('label')}
        >
          {children}
        </StyledMenuLabel>

        {suffix && (
          <StyledMenuSuffix
            collapsed={collapsed}
            transitionDuration={transitionDuration}
            firstLevel={level === 0}
            className={classnames(menuClasses.suffix, sharedClasses)}
            rootStyles={getMenuItemStyles('suffix')}
          >
            {suffix}
          </StyledMenuSuffix>
        )}
      </MenuButton>
    </StyledMenuItem>
  );
};

// Memoized so static items skip re-rendering when an ancestor re-renders with
// referentially-stable props (context changes still update them as usual).
export const MenuItem = React.memo(React.forwardRef<HTMLLIElement, MenuItemProps>(MenuItemFR));

MenuItem.displayName = 'MenuItem';
