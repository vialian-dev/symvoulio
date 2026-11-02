# @symvoulio/ui

The UI kit used by Symvoulio. It contains design tokens, typography and font fallbacks, reusable controls and cards, application frame components, responsive navigation, collapsing banners, motion constants, and internationalization helpers. It also includes a minimal example and helpers for translation checks.

## App imports

The app declares `@symvoulio/ui` as a workspace dependency. Its Vite build compiles the package’s TypeScript sources with the app.

```tsx
import { Button, Card } from '@symvoulio/ui/components'
import { Wordmark } from '@symvoulio/ui/frame'
import { applyInitialTheme } from '@symvoulio/ui/theme'
import { fmt } from '@symvoulio/ui/i18n'
import '@symvoulio/ui/fonts'
```

Import the theme after Tailwind in the app’s stylesheet:

```css
@import "tailwindcss";
@import "@symvoulio/ui/theme.css";
```

## Theming

CSS variables provide semantic colours, fonts, shadows, radii and motion settings. Light is the default theme; the `dark` class on the root element switches the semantic colours. Call `applyInitialTheme` with the app’s storage key before rendering to restore the saved choice. `useThemeToggle` provides the current theme and a toggle that remembers changes. The theme supports reduced motion, script-specific font stacks and right-to-left layouts.
