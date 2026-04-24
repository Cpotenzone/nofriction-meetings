# noFriction Meetings — Design System

**Version:** 1.0.0  
**Last Updated:** 2026-04-24

---

## Color System

### Semantic Tokens (use these, not raw hex)

```css
:root {
  /* ─── Background ─── */
  --bg-primary: #1a1d29;        /* Main app background */
  --bg-secondary: #242837;      /* Cards, panels */
  --bg-tertiary: #2d3142;       /* Hover states, wells */
  --bg-elevated: #353a50;       /* Modals, dropdowns */
  --bg-surface: rgba(255, 255, 255, 0.03); /* Subtle surface */

  /* ─── Text ─── */
  --text-primary: #f0f1f5;      /* Headings, primary content */
  --text-secondary: #b0b5c9;    /* Body text, descriptions */
  --text-tertiary: #6b7194;     /* Placeholders, disabled */
  --text-inverse: #1a1d29;      /* Text on light backgrounds */

  /* ─── Accent ─── */
  --accent-primary: #6366f1;    /* Indigo — primary actions */
  --accent-primary-hover: #818cf8;
  --accent-secondary: #22d3ee;  /* Cyan — status indicators */
  --accent-success: #34d399;    /* Green — success, recording active */
  --accent-warning: #fbbf24;    /* Amber — warnings */
  --accent-danger: #ef4444;     /* Red — errors, destructive actions */

  /* ─── Border ─── */
  --border-subtle: rgba(255, 255, 255, 0.06);
  --border-default: rgba(255, 255, 255, 0.10);
  --border-strong: rgba(255, 255, 255, 0.15);

  /* ─── Recording State ─── */
  --recording-active: #ef4444;     /* Red pulse when recording */
  --recording-paused: #fbbf24;     /* Amber when paused */
  --recording-idle: var(--text-tertiary);
}
```

### Usage Rules
- Never use raw hex values in components. Always reference `var(--token)`.
- The accent-primary was previously `#7c3aed` (purple, an AI slop signal). Changed to `#6366f1` (indigo) for a more intentional, less generic feel.
- Background hierarchy: primary (darkest) → secondary → tertiary → elevated (lightest).

---

## Typography

```css
:root {
  /* ─── Font Family ─── */
  --font-primary: 'Inter', -apple-system, BlinkMacSystemFont, sans-serif;
  --font-mono: 'JetBrains Mono', 'SF Mono', 'Menlo', monospace;

  /* ─── Font Sizes ─── */
  --text-xs: 0.6875rem;    /* 11px — timestamps, badges */
  --text-sm: 0.8125rem;    /* 13px — secondary text, captions */
  --text-base: 0.875rem;   /* 14px — body text (desktop-optimized) */
  --text-md: 1rem;         /* 16px — prominent body, labels */
  --text-lg: 1.125rem;     /* 18px — section headers */
  --text-xl: 1.5rem;       /* 24px — page titles */
  --text-2xl: 2rem;        /* 32px — hero text (rare) */

  /* ─── Font Weights ─── */
  --weight-regular: 400;
  --weight-medium: 500;
  --weight-semibold: 600;
  --weight-bold: 700;

  /* ─── Line Heights ─── */
  --leading-tight: 1.25;
  --leading-normal: 1.5;
  --leading-relaxed: 1.75;
}
```

### Rules
- Body text is 14px (`--text-base`), not 16px. This is a desktop app, not a website.
- Never go below 11px (`--text-xs`).
- Headings: `--weight-semibold`. Body: `--weight-regular`. Labels: `--weight-medium`.

---

## Spacing Scale

```css
:root {
  --space-1: 0.25rem;   /* 4px */
  --space-2: 0.5rem;    /* 8px */
  --space-3: 0.75rem;   /* 12px */
  --space-4: 1rem;      /* 16px */
  --space-5: 1.25rem;   /* 20px */
  --space-6: 1.5rem;    /* 24px */
  --space-8: 2rem;      /* 32px */
  --space-10: 2.5rem;   /* 40px */
  --space-12: 3rem;     /* 48px */
}
```

---

## Border Radius

```css
:root {
  --radius-sm: 4px;      /* Buttons, inputs */
  --radius-md: 8px;      /* Cards, panels */
  --radius-lg: 12px;     /* Modals, large containers */
  --radius-xl: 16px;     /* Feature cards */
  --radius-full: 9999px; /* Badges, avatars */
}
```

---

## Shadows

```css
:root {
  --shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.3);
  --shadow-md: 0 4px 12px rgba(0, 0, 0, 0.4);
  --shadow-lg: 0 8px 24px rgba(0, 0, 0, 0.5);
  --shadow-glow: 0 0 20px rgba(99, 102, 241, 0.15); /* Accent glow */
}
```

---

## Animation

```css
:root {
  --transition-fast: 120ms ease;
  --transition-base: 200ms ease;
  --transition-slow: 350ms ease;
  --transition-spring: 500ms cubic-bezier(0.34, 1.56, 0.64, 1);
}
```

### Rules
- Hover states: `--transition-fast`.
- Panel transitions: `--transition-base`.
- Modal open/close: `--transition-slow`.
- Micro-interactions (recording pulse): `--transition-spring`.
- Never use `transition: all`. Always specify the property.

---

## Component Patterns

### Buttons
- **Primary:** `--accent-primary` background, `--text-primary` text
- **Secondary:** `--bg-tertiary` background, `--text-secondary` text
- **Danger:** `--accent-danger` background
- **Ghost:** transparent background, `--text-secondary` text, visible on hover
- All buttons: `--radius-sm`, `--space-2` vertical padding, `--space-4` horizontal

### Cards
- Background: `--bg-secondary`
- Border: `--border-subtle`
- Border radius: `--radius-md`
- Padding: `--space-4`
- Hover: `--border-default` + `--shadow-sm`

### Empty States
Every feature MUST have an empty state that includes:
1. An icon or illustration (not an emoji)
2. A descriptive headline ("No meetings recorded yet")
3. A supporting sentence ("Record your first meeting to see transcripts, insights, and summaries here.")
4. A primary action button ("Start Recording")

### Error States
Every async operation MUST have an error state that includes:
1. Error icon
2. What went wrong (plain language, not error codes)
3. A recovery action ("Retry" / "Check Settings" / "Contact Support")
4. The raw error in a collapsible detail section (for debugging)

---

## Navigation Model

### Primary Modes (4 max)
| Mode | Icon | Purpose |
|------|------|---------|
| **Live** | 🎙️ | Active recording, live transcript, meeting controls |
| **Library** | 📚 | Past meetings, search, review, rewind |
| **Intelligence** | 🧠 | AI chat, insights, data exploration |
| **Settings** | ⚙️ | Configuration, accounts, admin |

### Secondary (contextual)
- **Library** expands to: Meetings, Vault, Knowledge Base
- **Intelligence** expands to: Chat, Intel Dashboard, Prompt Studio
- **Settings** expands to: General, Transcription, AI, Integrations, Admin

### Rules
- Maximum 4 primary items in the sidebar at all times.
- Power-user features (Vault, Prompt Studio, Admin) live under parent categories, not at the top level.
- Current mode is always visually indicated (highlight + label visible).
- The "trunk test": cover everything except the nav. Can you still tell what section you're in? If not, the nav has failed.

---

## Accessibility Requirements

- **Keyboard navigation:** Every interactive element is focusable and operable via keyboard.
- **Focus indicators:** Visible focus ring (2px `--accent-primary`, 2px offset).
- **Color contrast:** Minimum 4.5:1 for body text, 3:1 for large text.
- **Touch targets:** Minimum 44px for interactive elements.
- **Screen readers:** Semantic HTML + ARIA landmarks for all major regions.
- **Motion:** Respect `prefers-reduced-motion`. Disable animations when set.
