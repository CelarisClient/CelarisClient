import type { ButtonHTMLAttributes, HTMLAttributes, ReactNode } from "react";

/* ----------------------------------------------------------------------------
   Buttons
   -------------------------------------------------------------------------- */

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "secondary" | "ghost";
  size?: "md" | "lg";
};

export function Button({
  variant = "secondary",
  size = "md",
  className = "",
  children,
  ...rest
}: ButtonProps) {
  const variantClass = variant === "primary" ? "btn--primary" : variant === "ghost" ? "btn--ghost" : "";
  const sizeClass = size === "lg" ? "btn--lg" : "";
  return (
    <button className={`btn ${variantClass} ${sizeClass} ${className}`.trim()} {...rest}>
      {children}
    </button>
  );
}

export const PrimaryButton = (p: Omit<ButtonProps, "variant">) => <Button variant="primary" {...p} />;
export const SecondaryButton = (p: Omit<ButtonProps, "variant">) => <Button variant="secondary" {...p} />;

/* ----------------------------------------------------------------------------
   Card
   -------------------------------------------------------------------------- */

export function Card({
  interactive = false,
  className = "",
  children,
  ...rest
}: {
  interactive?: boolean;
  className?: string;
  children: ReactNode;
} & HTMLAttributes<HTMLDivElement>) {
  return (
    <div className={`card ${interactive ? "card--interactive" : ""} ${className}`.trim()} {...rest}>
      {children}
    </div>
  );
}

/* ----------------------------------------------------------------------------
   StatusBadge
   -------------------------------------------------------------------------- */

type BadgeTone = "accent" | "indigo" | "success" | "warning" | "muted";

export function StatusBadge({
  tone = "muted",
  dot = false,
  children,
}: {
  tone?: BadgeTone;
  dot?: boolean;
  children: ReactNode;
}) {
  return (
    <span className={`badge badge--${tone}`}>
      {dot && <span className="dot" />}
      {children}
    </span>
  );
}

/* ----------------------------------------------------------------------------
   ProgressBar
   -------------------------------------------------------------------------- */

export function ProgressBar({ value }: { value: number | null }) {
  const indeterminate = value === null;
  return (
    <div className="progress">
      <div
        className={`progress__fill ${indeterminate ? "progress__fill--indeterminate" : ""}`}
        style={indeterminate ? undefined : { width: `${Math.max(2, Math.min(100, value))}%` }}
      />
    </div>
  );
}

/* ----------------------------------------------------------------------------
   SidebarItem
   -------------------------------------------------------------------------- */

export function SidebarItem({
  icon,
  label,
  active,
  onClick,
}: {
  icon: ReactNode;
  label: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <div className={`sidebar-item ${active ? "active" : ""}`} onClick={onClick}>
      {icon}
      <span>{label}</span>
    </div>
  );
}

/* ----------------------------------------------------------------------------
   Icons (inline stroke SVGs)
   -------------------------------------------------------------------------- */

/** Celaris brand mark: the world held aloft on a cradle of arms. */
export function CelarisLogo({ size = 22 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.7} strokeLinecap="round" strokeLinejoin="round">
      <circle cx="12" cy="9" r="5.4" />
      <ellipse cx="12" cy="9" rx="5.4" ry="2" />
      <ellipse cx="12" cy="9" rx="2" ry="5.4" />
      <path d="M4.5 15 Q12 21.5 19.5 15" />
      <path d="M7 13.6 L9 12.4 M17 13.6 L15 12.4" />
    </svg>
  );
}

const iconProps = {
  className: "icon",
  width: 18,
  height: 18,
  viewBox: "0 0 24 24",
  fill: "none",
  strokeWidth: 1.8,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
};

export const PlayIcon = () => (
  <svg {...iconProps}>
    <path d="M7 5l12 7-12 7V5z" />
  </svg>
);
export const ProfilesIcon = () => (
  <svg {...iconProps}>
    <rect x="3" y="4" width="18" height="6" rx="1.5" />
    <rect x="3" y="14" width="18" height="6" rx="1.5" />
  </svg>
);
export const ModsIcon = () => (
  <svg {...iconProps}>
    <path d="M12 2l8 4.5v9L12 20l-8-4.5v-9L12 2z" />
    <path d="M12 11l8-4.5M12 11v9M12 11L4 6.5" />
  </svg>
);
export const CosmeticsIcon = () => (
  <svg {...iconProps}>
    <path d="M6 3h12l3 5-9 13L3 8l3-5z" />
    <path d="M3 8h18M9 3l3 18M15 3l-3 18M8 8l4-5 4 5" />
  </svg>
);
export const CreditsIcon = () => (
  <svg {...iconProps}>
    <path d="M12 21s-7-4.3-9.5-8.4C.7 9.6 2 6 5.2 6c2 0 3.2 1.2 3.8 2.3C9.6 7.2 10.8 6 12.8 6 16 6 17.3 9.6 15.5 12.6 13 16.7 12 21 12 21z" />
  </svg>
);
export const HostingIcon = () => (
  <svg {...iconProps}>
    <path d="M6 16a4 4 0 0 1-.5-7.97 5.5 5.5 0 0 1 10.7-1.2A4.5 4.5 0 0 1 17.5 16H6z" />
    <path d="M12 12v5M9.5 14.5 12 12l2.5 2.5" />
  </svg>
);
export const FriendsIcon = () => (
  <svg {...iconProps}>
    <circle cx="9" cy="8" r="3" />
    <path d="M3.5 19a5.5 5.5 0 0 1 11 0" />
    <path d="M16 5.5a3 3 0 0 1 0 5.5M17 19a5.5 5.5 0 0 0-3-4.9" />
  </svg>
);
export const NewsIcon = () => (
  <svg {...iconProps}>
    <path d="M4 5h13v14H5a1 1 0 0 1-1-1V5z" />
    <path d="M17 8h3v9a2 2 0 0 1-2 2" />
    <path d="M7 8h7M7 11h7M7 14h5" />
  </svg>
);
export const ServerIcon = () => (
  <svg {...iconProps}>
    <rect x="3" y="4" width="18" height="6" rx="1.5" />
    <rect x="3" y="14" width="18" height="6" rx="1.5" />
    <path d="M7 7h.01M7 17h.01" />
  </svg>
);
export const WardrobeIcon = () => (
  <svg {...iconProps}>
    <path d="M12 3a2 2 0 0 0-2 2c0 .9.6 1.6 1.4 1.9L3 12.5V14h18v-1.5l-8.4-5.6c.8-.3 1.4-1 1.4-1.9a2 2 0 0 0-2-2z" />
    <path d="M4 14v5h16v-5" />
  </svg>
);
export const SettingsIcon = () => (
  <svg {...iconProps}>
    <circle cx="12" cy="12" r="3" />
    <path d="M19 12a7 7 0 0 0-.13-1.3l2-1.5-2-3.4-2.3 1a7 7 0 0 0-2.27-1.3L13.7 2h-3.4l-.33 2.5a7 7 0 0 0-2.27 1.3l-2.3-1-2 3.4 2 1.5A7 7 0 0 0 5 12a7 7 0 0 0 .13 1.3l-2 1.5 2 3.4 2.3-1a7 7 0 0 0 2.27 1.3l.33 2.5h3.4l.33-2.5a7 7 0 0 0 2.27-1.3l2.3 1 2-3.4-2-1.5A7 7 0 0 0 19 12z" />
  </svg>
);
