# noFriction — Design System

**Version:** 1.2.0  
**Last Updated:** 2026-10-09

> v1.2.0 (the Ive pass, after `docs/design/FADELL_AUDIT.md`): one window
> with Record · Recordings · Chat, sentence case everywhere, one vocabulary.
> The hazard-yellow palette and Inter stay.

> v1.1.0 records the design system the app actually ships: the hazard-yellow
> "command center" brand implemented in `src/App.css`. The previous indigo
> palette documented here was never what the app rendered.

---

## Color System

### Semantic Tokens (use these, not raw hex)

```css
:root {
  /* ─── Brand: hazard yellow on matte black ─── */
  --hazard-yellow: #facc15;     /* Primary accent — actions, active nav, focus */
  --hazard-yellow-dim: rgba(250, 204, 21, 0.5);
  --accent-primary: var(--hazard-yellow);

  /* ─── Background ─── */
  --bg-main: #050505;           /* Main app background */
  --bg-card: #0d0d0d;           /* Cards, panels */
  --matte-black: #0a0a0a;
  --tactical-gray: #171717;     /* Hover states, wells */

  /* ─── Text ─── */
  --text-primary: #f5f5f5;      /* Headings, primary content */
  --text-secondary: #a3a3a3;    /* Body text, descriptions */

  /* ─── Status ─── */
  --accent-success: #34d399;    /* Success */
  --accent-warning: var(--hazard-yellow);
  --accent-danger: #ef4444;     /* Errors, destructive, recording pulse */

  /* ─── Border ─── */
  --border-color: #262626;
  --hud-border: 1px solid #262626;
  --hud-border-bright: 1px solid #404040;
}
```

### Usage Rules
- Never use raw hex values in components. Always reference `var(--token)`.
- One accent: hazard yellow. Red is reserved for recording/destructive; green for success. No purple/indigo anywhere (AI-slop signal; legacy `--accent-purple` is aliased to yellow for backward compatibility only).
- Background hierarchy: `--bg-main` (darkest) → `--bg-card` → `--tactical-gray` (lightest).

---

## Iconography

- **Never use emoji as UI icons.** Emoji render inconsistently across
  platforms and can't follow color tokens. Use the inline SVG stroke set in
  `src/components/icons.tsx` (24×24 viewBox, `currentColor`, round caps).
- Nav/control icons: 14–17px. Empty-state/hero icons: 40–48px at
  `strokeWidth={1.5}`.
- Add new icons to `icons.tsx`; don't inline one-off SVGs in components.

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
- **Sentence case everywhere.** No `text-transform: uppercase`, no tracked
  all-caps labels. Caps read as a dashboard; this is a notebook.
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

### Three views (top bar)
| View | Purpose |
|------|---------|
| **Record** | Idle: a title, the privacy promise, one Record button. Recording: the capture bar (time left, Mark, Capture screen), the live transcript, the screens. No AI runs here. |
| **Recordings** | One search field (⌘K), Notebook chips, the list by day; the open recording's title block and **Rewind · Notes · Links · Review guide**. |
| **Chat** | Ask your recordings, scoped to all, a notebook, a topic or one recording, with citations. |

Right side of the bar: **Stop** while recording, and the Settings gear.
Settings has five sections: Recording, Transcription, AI, Subscription
(`mas` only), About. Help is one document, `docs/USER_GUIDE.md`.

### Rules
- Three views. Nothing goes under a MORE menu; if it can't earn a place in
  the three views or Settings, it is cut.
- One control per act: one Record, one Stop, one search, one Delete with
  Undo, one Strike from the record.
- Banners live in the one slot under the top bar, never over it.
- The menu bar, tray and window use the same words.

## Vocabulary

| Concept | Say | Never |
|---------|-----|-------|
| The thing you made | recording | meeting (in UI), capture, session, file |
| Start / stop | Record / Stop | Start capture, Stop now |
| The list | Recordings | Rewind (as a tab), Knowledge Base |
| The timeline inside a recording | Rewind | |
| AI notes | Notes; verbs **Make notes** / **Make again** | Summarize, Generate, Regenerate, AI notes |
| Marks | **Mark** (verb), **Marks** (list) | Markers, marked moments |
| Grouping | Notebook | class, course, tag |
| Type | Meeting · Class · Personal | |
| Review material | Review guide (Study guide for a Class) | Study (for every type) |
| Screens (Mac) / Photos (iOS) | Capture screen | Snap |
| AI | AI (section), endpoint (the URL), connection (saved) | AI Engine, provider (in UI) |
| Settings / Help | Settings, Help | System, Overlay, Admin Console, Docs |

---

## Accessibility Requirements

- **Keyboard navigation:** Every interactive element is focusable and operable via keyboard.
- **Focus indicators:** Visible focus ring (2px `--accent-primary`, 2px offset).
- **Color contrast:** Minimum 4.5:1 for body text, 3:1 for large text.
- **Touch targets:** Minimum 44px for interactive elements.
- **Screen readers:** Semantic HTML + ARIA landmarks for all major regions.
- **Motion:** Respect `prefers-reduced-motion`. Disable animations when set.
