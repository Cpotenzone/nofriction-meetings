// noFriction — icon set
// Inline SVG stroke icons (currentColor) so every glyph inherits the theme.
// Replaces emoji glyphs, which render inconsistently across platforms and
// can't follow the design system's color tokens.

import React from 'react';

interface IconProps {
    size?: number;
    strokeWidth?: number;
    className?: string;
}

const base = (size: number, className?: string) => ({
    width: size,
    height: size,
    viewBox: '0 0 24 24',
    fill: 'none',
    stroke: 'currentColor',
    strokeLinecap: 'round' as const,
    strokeLinejoin: 'round' as const,
    className,
    'aria-hidden': true,
});

export const GearIcon: React.FC<IconProps> = ({ size = 16, strokeWidth = 2, className }) => (
    <svg {...base(size, className)} strokeWidth={strokeWidth}>
        <circle cx="12" cy="12" r="3" />
        <path d="M19.4 15a1.7 1.7 0 0 0 .34 1.87l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.87-.34 1.7 1.7 0 0 0-1 1.55V21a2 2 0 1 1-4 0v-.09a1.7 1.7 0 0 0-1-1.55 1.7 1.7 0 0 0-1.87.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.7 1.7 0 0 0 .34-1.87 1.7 1.7 0 0 0-1.55-1H3a2 2 0 1 1 0-4h.09a1.7 1.7 0 0 0 1.55-1 1.7 1.7 0 0 0-.34-1.87l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.7 1.7 0 0 0 1.87.34h.09a1.7 1.7 0 0 0 1-1.55V3a2 2 0 1 1 4 0v.09a1.7 1.7 0 0 0 1 1.55 1.7 1.7 0 0 0 1.87-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.7 1.7 0 0 0-.34 1.87v.09a1.7 1.7 0 0 0 1.55 1H21a2 2 0 1 1 0 4h-.09a1.7 1.7 0 0 0-1.55 1z" />
    </svg>
);

export const SparkleIcon: React.FC<IconProps> = ({ size = 16, strokeWidth = 2, className }) => (
    <svg {...base(size, className)} strokeWidth={strokeWidth}>
        <path d="M12 3l1.9 5.1L19 10l-5.1 1.9L12 17l-1.9-5.1L5 10l5.1-1.9z" />
        <path d="M19 15l.9 2.1L22 18l-2.1.9L19 21l-.9-2.1L16 18l2.1-.9z" />
    </svg>
);

export const MicIcon: React.FC<IconProps> = ({ size = 16, strokeWidth = 2, className }) => (
    <svg {...base(size, className)} strokeWidth={strokeWidth}>
        <rect x="9" y="2" width="6" height="12" rx="3" />
        <path d="M5 10a7 7 0 0 0 14 0M12 17v4M8 21h8" />
    </svg>
);

export const SearchIcon: React.FC<IconProps> = ({ size = 16, strokeWidth = 2, className }) => (
    <svg {...base(size, className)} strokeWidth={strokeWidth}>
        <circle cx="11" cy="11" r="7" />
        <path d="M21 21l-4.35-4.35" />
    </svg>
);

export const CalendarIcon: React.FC<IconProps> = ({ size = 16, strokeWidth = 2, className }) => (
    <svg {...base(size, className)} strokeWidth={strokeWidth}>
        <rect x="3" y="5" width="18" height="16" rx="2" />
        <path d="M8 3v4M16 3v4M3 10h18" />
    </svg>
);

export const CheckIcon: React.FC<IconProps> = ({ size = 16, strokeWidth = 2, className }) => (
    <svg {...base(size, className)} strokeWidth={strokeWidth}>
        <path d="M20 6L9 17l-5-5" />
    </svg>
);

export const WarningIcon: React.FC<IconProps> = ({ size = 16, strokeWidth = 2, className }) => (
    <svg {...base(size, className)} strokeWidth={strokeWidth}>
        <path d="M10.3 3.9L1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z" />
        <path d="M12 9v4" />
        <circle cx="12" cy="17" r="0.5" fill="currentColor" />
    </svg>
);

export const QuestionIcon: React.FC<IconProps> = ({ size = 16, strokeWidth = 2, className }) => (
    <svg {...base(size, className)} strokeWidth={strokeWidth}>
        <path d="M9.2 9a2.8 2.8 0 0 1 5.5.9c0 1.8-2.7 2.3-2.7 3.6" />
        <circle cx="12" cy="17" r="0.5" fill="currentColor" />
    </svg>
);

export const TrashIcon: React.FC<IconProps> = ({ size = 16, strokeWidth = 2, className }) => (
    <svg {...base(size, className)} strokeWidth={strokeWidth}>
        <path d="M3 6h18M8 6V4a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1v2M5 6l1 14a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2l1-14" />
        <path d="M10 11v6M14 11v6" />
    </svg>
);

/** Brain — Chat's empty state */
export const BrainIcon: React.FC<IconProps> = ({ size = 16, strokeWidth = 2, className }) => (
    <svg {...base(size, className)} strokeWidth={strokeWidth}>
        <path d="M12 4.5a3 3 0 0 0-5.8 1A3.5 3.5 0 0 0 4 12a3.5 3.5 0 0 0 1.6 6A3 3 0 0 0 12 19.5V4.5z" />
        <path d="M12 4.5a3 3 0 0 1 5.8 1A3.5 3.5 0 0 1 20 12a3.5 3.5 0 0 1-1.6 6A3 3 0 0 1 12 19.5" />
    </svg>
);

/** Display — a monitor on a stand */
export const DisplayIcon: React.FC<IconProps> = ({ size = 16, strokeWidth = 2, className }) => (
    <svg {...base(size, className)} strokeWidth={strokeWidth}>
        <rect x="2.5" y="4" width="19" height="12.5" rx="1.5" />
        <path d="M8.5 20.5h7M12 16.5v4" />
    </svg>
);

/** Window — app window with title bar */
export const WindowIcon: React.FC<IconProps> = ({ size = 16, strokeWidth = 2, className }) => (
    <svg {...base(size, className)} strokeWidth={strokeWidth}>
        <rect x="3" y="4.5" width="18" height="15" rx="2" />
        <path d="M3 9h18" />
        <circle cx="6" cy="6.75" r="0.4" fill="currentColor" />
        <circle cx="8" cy="6.75" r="0.4" fill="currentColor" />
    </svg>
);

/** Capture screen — camera */
export const CameraIcon: React.FC<IconProps> = ({ size = 16, strokeWidth = 2, className }) => (
    <svg {...base(size, className)} strokeWidth={strokeWidth}>
        <path d="M4 8h3l1.5-2.5h7L17 8h3a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V9a1 1 0 0 1 1-1z" />
        <circle cx="12" cy="13" r="3.5" />
    </svg>
);

/** Star — a moment marked Important (study markers) */
export const StarIcon: React.FC<IconProps> = ({ size = 16, strokeWidth = 2, className }) => (
    <svg {...base(size, className)} strokeWidth={strokeWidth}>
        <path d="M12 3.5l2.6 5.4 5.9.8-4.3 4.1 1 5.8L12 16.8l-5.2 2.8 1-5.8-4.3-4.1 5.9-.8z" />
    </svg>
);

/** Pencil — a moment marked On the test (study markers) */
export const PencilIcon: React.FC<IconProps> = ({ size = 16, strokeWidth = 2, className }) => (
    <svg {...base(size, className)} strokeWidth={strokeWidth}>
        <path d="M15.5 4.5l4 4L8 20H4v-4z" />
        <path d="M13 7l4 4" />
    </svg>
);

/** Bookmark flag — "mark this moment" */
export const MarkIcon: React.FC<IconProps> = ({ size = 16, strokeWidth = 2, className }) => (
    <svg {...base(size, className)} strokeWidth={strokeWidth}>
        <path d="M6 3.5h12v17l-6-4-6 4z" />
    </svg>
);

/** Notebook — a group of recordings */
export const BookIcon: React.FC<IconProps> = ({ size = 16, strokeWidth = 2, className }) => (
    <svg {...base(size, className)} strokeWidth={strokeWidth}>
        <path d="M4 4.5A1.5 1.5 0 0 1 5.5 3H19a1 1 0 0 1 1 1v15a1 1 0 0 1-1 1H5.5A1.5 1.5 0 0 1 4 18.5z" />
        <path d="M4 17.5A1.5 1.5 0 0 1 5.5 16H20" />
        <path d="M8 7h8" />
    </svg>
);

/** Recording type (Meeting · Class · Personal) — a tag */
export const TagIcon: React.FC<IconProps> = ({ size = 16, strokeWidth = 2, className }) => (
    <svg {...base(size, className)} strokeWidth={strokeWidth}>
        <path d="M3 11V4a1 1 0 0 1 1-1h7l10 10-8 8z" />
        <circle cx="7.5" cy="7.5" r="1" fill="currentColor" />
    </svg>
);

/** Play — audio playback */
export const PlayIcon: React.FC<IconProps> = ({ size = 16, strokeWidth = 2, className }) => (
    <svg {...base(size, className)} strokeWidth={strokeWidth}>
        <path d="M7 4.5v15l12-7.5z" />
    </svg>
);

/** Chevron down — a menu opens */
export const ChevronDownIcon: React.FC<IconProps> = ({ size = 16, strokeWidth = 2, className }) => (
    <svg {...base(size, className)} strokeWidth={strokeWidth}>
        <path d="M6 9l6 6 6-6" />
    </svg>
);

/** Padlock — stays on this device */
export const LockIcon: React.FC<IconProps> = ({ size = 16, strokeWidth = 2, className }) => (
    <svg {...base(size, className)} strokeWidth={strokeWidth}>
        <rect x="5" y="11" width="14" height="10" rx="2" />
        <path d="M8 11V7a4 4 0 0 1 8 0v4" />
    </svg>
);
