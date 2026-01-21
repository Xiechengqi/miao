# Build & Deployment

## Local Development Build

### Prerequisites
- Rust toolchain (1.88.0+)
- Node.js 20+ and pnpm 9+
- curl for downloading embedded binaries

### Build Commands

```bash
# Complete build (frontend + embedded binaries + Rust)
./build.sh

# Build output location
# Binary: target/release/miao-rust
# Frontend: public/
# Embedded binaries: embedded/
```

### Build Process Details

The `build.sh` script performs the following steps:

1. **Architecture Detection**: Automatically detects x86_64 (amd64) or aarch64 (arm64)
2. **Download Embedded Binaries**: Downloads gotty, sy, and sing-box if not already present
3. **Build Frontend**: Runs `pnpm install` and `pnpm run build` in frontend directory
4. **Copy Static Assets**: Copies frontend/out/ to public/ for Rust embedding
5. **Build Rust Binary**: Compiles Rust with embedded static assets

### Deployment

```bash
# Copy binary to deployment location
cp target/release/miao-rust /root/miao/miao

# Start service in background
cd /root/miao && nohup ./miao > miao.log 2>&1 &

# Check service status
ps aux | grep miao | grep -v grep

# View logs
tail -f /root/miao/miao.log

# Stop service
pkill -f '/root/miao/miao'
```

### CI Build (Cross-compilation)

For GitHub Actions or cross-compilation:

```bash
# Build for specific architecture
./build-ci.sh amd64   # or arm64

# Requires additional setup:
# - cargo-zigbuild for cross-compilation
# - Go toolchain for building sing-box
# - musl targets: x86_64-unknown-linux-musl, aarch64-unknown-linux-musl
```

---

<role>
You are an expert frontend engineer, UI/UX designer, visual design specialist, and typography expert. Your goal is to help the user integrate a design system into an existing codebase in a way that is visually consistent, maintainable, and idiomatic to their tech stack.

Before proposing or writing any code, first build a clear mental model of the current system:
- Identify the tech stack (e.g. React, Next.js, Vue, Tailwind, shadcn/ui, etc.).
- Understand the existing design tokens (colors, spacing, typography, radii, shadows), global styles, and utility patterns.
- Review the current component architecture (atoms/molecules/organisms, layout primitives, etc.) and naming conventions.
- Note any constraints (legacy CSS, design library in use, performance or bundle-size considerations).

Ask the user focused questions to understand the user's goals. Do they want:
- a specific component or page redesigned in the new style,
- existing components refactored to the new system, or
- new pages/features built entirely in the new style?

Once you understand the context and scope, do the following:
- Propose a concise implementation plan that follows best practices, prioritizing:
  - centralizing design tokens,
  - reusability and composability of components,
  - minimizing duplication and one-off styles,
  - long-term maintainability and clear naming.
- When writing code, match the user’s existing patterns (folder structure, naming, styling approach, and component patterns).
- Explain your reasoning briefly as you go, so the user understands *why* you’re making certain architectural or design choices.

Always aim to:
- Preserve or improve accessibility.
- Maintain visual consistency with the provided design system.
- Leave the codebase in a cleaner, more coherent state than you found it.
- Ensure layouts are responsive and usable across devices.
- Make deliberate, creative design choices (layout, motion, interaction details, and typography) that express the design system’s personality instead of producing a generic or boilerplate UI.

</role>

<design-system>
# Design Style: Corporate Trust

## 1. Design Philosophy
This style embodies the **modern enterprise SaaS aesthetic** — professional yet approachable, sophisticated yet friendly. It draws inspiration from tech unicorns and high-growth startups that have successfully humanized the corporate experience. The design rejects the cold, sterile formality of traditional corporate websites in favor of a warm, confident, and inviting presence.

**Core Principles:**
- **Trustworthy Yet Vibrant**: Establishes credibility through clean structure and professional typography while maintaining visual energy through vibrant gradients and colorful accents
- **Dimensional Depth**: Uses isometric perspectives, soft colored shadows, and subtle 3D transforms to create visual interest and break free from flat design
- **Refined Elegance**: Every element is polished with attention to micro-interactions, smooth transitions, and sophisticated hover states
- **Purposeful Gradients**: Indigo-to-violet gradients serve as the visual signature, used strategically in headlines, buttons, and decorative elements
- **Professional Polish**: Generous white space, consistent spacing rhythms, and crisp typography create a premium, enterprise-ready feel

**Keywords**: Trustworthy, Vibrant, Polished, Dimensional, Modern, Approachable, Enterprise-Ready, Elegant

**Visual DNA**: The unmistakable signature of this style comes from:
1. **Colored Shadows**: Soft shadows with blue/purple tints instead of neutral grays
2. **Isometric Elements**: Subtle 3D transforms (rotate-x, rotate-y) on decorative cards and visualizations
3. **Gradient Text**: Strategic use of gradient text for emphasis in headlines
4. **Soft Blobs**: Large, blurred gradient orbs in the background for atmospheric depth
5. **Elevated Cards**: White cards that lift on hover with enhanced shadows
6. **Dual-Tone Palette**: Indigo (primary) + Violet (secondary) creating a cohesive gradient spectrum

## 2. Design Token System

### Colors (Light Mode)
*   **Background**: `#F8FAFC` (Slate 50) - A very subtle cool grey/white base.
*   **Foreground (Surface)**: `#FFFFFF` (White) - For cards and raised elements.
*   **Primary**: `#4F46E5` (Indigo 600) - The core brand color. Vibrant blue-purple.
*   **Secondary**: `#7C3AED` (Violet 600) - For gradients and accents.
*   **Text Main**: `#0F172A` (Slate 900) - High contrast, sharp.
*   **Text Muted**: `#64748B` (Slate 500) - For supporting text.
*   **Accent/Success**: `#10B981` (Emerald 500) - For positive indicators.
*   **Border**: `#E2E8F0` (Slate 200) - Subtle separation.

### Typography
*   **Font Family**: `Plus Jakarta Sans` — A geometric sans-serif with friendly rounded terminals that perfectly balances professional authority with modern approachability. Its clean letterforms ensure excellent readability while maintaining visual warmth.
*   **Scaling**: Major Third (1.250) scale provides substantial hierarchy without overwhelming the layout
*   **Font Weights**:
    *   **Display/Headings**: ExtraBold (800) for hero headlines, Bold (700) for section headings
    *   **Subheadings**: SemiBold (600) for card titles and emphasis
    *   **Body Text**: Regular (400) for paragraphs, Medium (500) for navigation and labels
*   **Line Heights**:
    *   Headlines: 1.1 (tight tracking for impact)
    *   Body Text: 1.6-1.7 (relaxed for readability)
*   **Letter Spacing**: Tight tracking (-0.02em) on large headlines for modern polish
*   **Responsive Type Scale**:
    *   Mobile: text-2xl to text-4xl for h1
    *   Desktop: text-4xl to text-6xl for h1
    *   Progressive scaling ensures legibility across all devices

### Radius & Border
*   **Radius**: `rounded-xl` (12px) for cards and `rounded-lg` (8px) for inputs. Buttons are `rounded-full` or `rounded-lg`.
*   **Borders**: Thin, 1px borders using the `Border` token.

### Shadows & Effects
This is where the design truly shines. **Colored shadows** replace neutral grays to reinforce the brand palette:

*   **Default Card Shadow**: `0 4px 20px -2px rgba(79, 70, 229, 0.1)` — Soft blue-tinted base elevation
*   **Hover Card Shadow**: `0 10px 25px -5px rgba(79, 70, 229, 0.15), 0 8px 10px -6px rgba(79, 70, 229, 0.1)` — Multi-layer depth on interaction
*   **Button Shadow**: `0 4px 14px 0 rgba(79, 70, 229, 0.3)` — Strong presence for primary CTAs
*   **Glow Effects**: Numbered badges use `shadow-[0_0_20px_rgba(79,70,229,0.5)]` for ethereal glow
*   **Background Blobs**: Large gradient orbs with 3xl blur create atmospheric depth without distraction
    *   `blur-3xl filter` combined with low opacity (20-50%)
    *   Positioned absolutely to create layered depth
*   **Gradients**:
    *   **Primary Gradient**: `from-indigo-600 to-violet-600` — Used for buttons and active states
    *   **Text Gradient**: Combined with `bg-clip-text text-transparent` for striking headlines
    *   **Background Gradients**: Subtle `from-indigo-100 to-violet-100` for container backgrounds
    *   **Final CTA Background**: `from-indigo-900 to-indigo-950` for dramatic dark section

## 3. Component Stylings

### Buttons
*   **Primary**: Gradient background (Indigo to Violet). `rounded-full` or `rounded-lg`. White text. Slight shadow. Transition: Lift (`-translate-y-0.5`) and increase shadow on hover.
*   **Secondary**: White background, Border `E2E8F0`, Text `Slate 700`. Hover: `bg-slate-50` and darker border.

### Cards
*   **Base**: White background, `rounded-xl`, `border border-slate-100`, `shadow-soft`.
*   **Behavior**: On hover, slight lift and increased shadow intensity.
*   **Feature Cards**: May feature an icon in a soft-colored circle (bg-indigo-50 text-indigo-600).

### Inputs
*   **Style**: `bg-white`, `border-slate-200`, `rounded-lg`.
*   **Focus**: `ring-2 ring-indigo-500 ring-offset-1` and `border-indigo-500`.
*   **Label**: `text-sm font-semibold text-slate-700`.

## 4. Non-Generic Bold Choices

The Corporate Trust aesthetic stands out through deliberate, sophisticated design decisions:

### Isometric Depth & 3D Transforms
*   **Hero Card**: `perspective-[2000px]` parent with `rotate-x-[5deg] rotate-y-[-12deg]` child creates subtle isometric effect
*   **Hover Transforms**: `hover:rotate-x-[2deg] hover:rotate-y-[-8deg]` — Subtle 3D movement on interaction
*   **Feature Cards**: Alternating `rotate-y-[6deg]` and `rotate-y-[-6deg]` based on layout position
*   **Benefit Visualization**: `rotate-x-6 rotate-y-12 transform` on gradient container for dramatic depth

### Strategic Gradient Usage
*   **Split Headlines**: First 3 words in standard color, remaining words in gradient for visual hierarchy
*   **Gradient Buttons**: Full background gradient with hover lift (`-translate-y-0.5`)
*   **Badge Elements**: NEW badge with solid indigo background inside gradient-ringed container
*   **Final CTA**: White button on dark gradient background creates dramatic contrast

### Atmospheric Background Elements
*   **Blur Orbs**: Large (400-600px) circular gradients with heavy blur positioned absolutely
*   **Layered Positioning**: Multiple blobs at different z-indexes create depth
*   **Subtle Animation**: `animate-pulse duration-[4000ms]` on floating cards for gentle movement

### Elevated Card System
*   **Default State**: Soft colored shadow with subtle border
*   **Hover State**: Lift effect (`-translate-y-1`) combined with enhanced shadow
*   **Transition**: Smooth `duration-200` for professional polish
*   **Pricing Highlight**: Center card uses `md:scale-105` with special ring styling

### Micro-Interactions
*   **Arrow Icons**: `transition-transform group-hover:translate-x-1` for directional feedback
*   **Image Zoom**: `group-hover:scale-105` on blog images with overlay fade-in
*   **Chevron Rotation**: `group-open:rotate-180` for FAQ accordions
*   **Button Lift**: Subtle upward movement on hover reinforces clickability

## 5. Spacing & Layout
*   **Container**: `max-w-7xl` (1280px) provides spacious, enterprise-appropriate width
*   **Padding**: Responsive padding with `px-4 sm:px-6` pattern for consistent gutters
*   **Vertical Rhythm**:
    *   Mobile: `py-16` (64px)
    *   Tablet: `sm:py-20` (80px)
    *   Desktop: `lg:py-24` (96px)
*   **Section Spacing**: Generous white space between sections creates breathing room
*   **Grid Strategy**:
    *   Hero: Two-column `lg:grid-cols-2` with text-first approach
    *   Features: Alternating zig-zag with `lg:flex-row` and `lg:flex-row-reverse`
    *   Pricing: Three-column `md:grid-cols-3` with center emphasis
    *   Stats: Four-column `md:grid-cols-4` for metric display
*   **Responsive Breakpoints**:
    *   Mobile-first approach with progressive enhancement
    *   sm: 640px, md: 768px, lg: 1024px, xl: 1280px
*   **Text Width Constraints**: `max-w-xl` or `max-w-2xl` on paragraphs to maintain 60-75 character line lengths

## 6. Animation & Transitions
*   **Philosophy**: "Refined Motion" — Smooth, professional, never jarring
*   **Base Transition**: `transition-all duration-200` for general interactive elements
*   **Long Transitions**: `duration-500` for image zooms and complex animations
*   **Hover Effects**:
    *   Cards: Combine `hover:-translate-y-1` with shadow enhancement
    *   Buttons: `hover:-translate-y-0.5` for subtle lift
    *   Icons: `transition-transform group-hover:translate-x-1` for directional cues
*   **Easing**: Default `ease-out` for natural deceleration
*   **Pulse Animation**: `animate-pulse duration-[4000ms]` on decorative floating elements for gentle breathing effect
*   **State Changes**: Smooth color transitions on links and buttons reinforce interactivity

## 7. Iconography
*   **Library**: `lucide-react` for consistent, modern icon system
*   **Style**:
    *   Default stroke width: `2px` (standard)
    *   Size: `h-4 w-4` for inline icons, `h-5 w-5` or `h-6 w-6` for featured icons
    *   Joins: Rounded for friendliness
*   **Color Treatment**:
    *   **Badge Icons**: Icon in `text-indigo-600` on `bg-indigo-100` container
    *   **Navigation Icons**: Inherit text color, transition on hover
    *   **Social Icons**: `text-slate-400 hover:text-indigo-400`
*   **Icon Containers**:
    *   Small badges: `h-12 w-12 rounded-xl` with soft background
    *   Large features: `h-14 w-14 rounded-xl` for prominent sections
    *   Circular: `rounded-full` for avatars or status indicators
*   **Accessibility**: Icons are decorative with proper text alternatives or hidden from screen readers when paired with text

## 8. Responsive Strategy
*   **Mobile-First Philosophy**: Design begins at 375px width, progressively enhances
*   **Touch Targets**: Minimum 44x44px for all interactive elements (buttons, links)
*   **Typography Scaling**:
    *   Headlines reduce from `text-6xl` (desktop) to `text-4xl` (mobile)
    *   Body text maintains readability at `text-base` with responsive line heights
*   **Layout Adaptations**:
    *   Two-column layouts stack to single column on mobile
    *   Navigation collapses to essential items (login hidden on mobile)
    *   Pricing cards stack vertically with equal width
    *   Footer columns stack progressively (4 col → 2 col → 1 col)
*   **Spacing Compression**: Padding and margins reduce proportionally on smaller screens
*   **Image Optimization**: Aspect ratios maintained, sizes adapt to container width
*   **Horizontal Scrolling**: Never required; all content fits viewport width
*   **Visual Hierarchy Preserved**: Even on mobile, clear distinction between heading levels maintained

## 9. Accessibility & Best Practices
*   **Color Contrast**: All text meets WCAG AA standards
    *   Slate 900 on Slate 50 background: AAA compliant
    *   White text on Indigo 900 background: AAA compliant
    *   Link colors tested for 4.5:1 minimum ratio
*   **Focus States**:
    *   Visible ring on all interactive elements: `focus-visible:ring-2 focus-visible:ring-indigo-500`
    *   Ring offset for clarity: `focus-visible:ring-offset-2`
    *   Never remove focus indicators
*   **Semantic HTML**:
    *   Proper heading hierarchy (h1 → h2 → h3)
    *   Native `<button>` elements for interactive actions
    *   `<nav>` for navigation, `<footer>` for footer
    *   Details/summary for FAQ accordions
*   **Image Alt Text**: Descriptive alternatives for all images
*   **Interactive States**:
    *   Hover: Visual feedback on all clickable elements
    *   Active: Subtle state change on click
    *   Disabled: Reduced opacity with `pointer-events-none`
*   **Motion Preferences**: Consider `prefers-reduced-motion` for users sensitive to animation
*   **Screen Reader Support**: Proper ARIA labels where semantic HTML insufficient
</design-system>

---

# Testing & Quality Assurance

## Automated UI Testing

Miao includes an automated UI test suite to verify the complete user experience from login to logout. The test suite uses `agent-browser` to simulate real user interactions.

### Quick Start

```bash
# Run the full UI test suite
./tests/ui-test.sh
```

### Test Coverage

The UI test suite covers the following scenarios:

1. **Homepage Redirect** - Unauthenticated users redirected to login
2. **Login Page Elements** - Password input and login button present
3. **System Initialization** - Automatic setup if not initialized
4. **Login Functionality** - Successful login with valid credentials
5. **Token Storage** - JWT token saved to localStorage
6. **Dashboard Content** - System metrics and navigation displayed
7. **Navigation to Proxies** - Proxies page loads correctly
8. **Navigation to Sync** - Sync page loads correctly
9. **Return to Dashboard** - Dashboard home navigation works
10. **Logout Functionality** - Successful logout and redirect
11. **Token Cleared** - Authentication token removed after logout
12. **Protected Route Access** - Unauthorized access redirected to login

### Configuration

Configure the test suite using environment variables:

```bash
# Base URL (default: http://localhost:6161)
export MIAO_BASE_URL="http://localhost:8080"

# Test password (default: admin123)
export MIAO_TEST_PASSWORD="mypassword"

# Screenshot directory (default: ./test-screenshots)
export MIAO_SCREENSHOT_DIR="/tmp/test-results"

# Headless mode (default: true)
export MIAO_HEADLESS="false"  # Show browser during tests
```

### Test Output

The test suite generates:

1. **Console Output** - Real-time test progress with color-coded results
2. **Screenshots** - Saved to `test-screenshots/` directory for each major step
3. **Test Report** - Summary of passed/failed tests at the end

Example output:

```
======================================
  Miao UI Automated Test Suite
======================================

[INFO] Checking if Miao server is running at http://localhost:6161...
[PASS] Server is running
[INFO] Initializing browser session...
[PASS] Browser session initialized
[INFO] Test 1: Homepage should redirect to login when not authenticated
[PASS] Homepage correctly redirects to login
[INFO] Test 2: Login page should have all required elements
[PASS] Login page has password input and login button
...

======================================
     Miao UI Test Report
======================================

Base URL: http://localhost:6161
Test Password: admin123
Screenshots: ./test-screenshots

Tests Passed: 12
Tests Failed: 0

✓ All tests passed!
```

### Prerequisites

- Miao server must be running on the configured port
- `agent-browser` must be installed and available in PATH
- `curl` and `jq` must be installed for API testing

### CI/CD Integration

Add to your GitHub Actions workflow:

```yaml
- name: Run UI Tests
  run: |
    # Start Miao server in background
    ./target/release/miao-rust &
    MIAO_PID=$!

    # Wait for server to start
    sleep 5

    # Run UI tests
    ./tests/ui-test.sh

    # Cleanup
    kill $MIAO_PID
```

### Extending Tests

To add new test cases, edit `tests/ui-test.sh`:

```bash
# Add a new test function
test_my_new_feature() {
    log_info "Test N: Description of test"

    # Test implementation
    agent-browser click @e1 > /dev/null 2>&1

    # Validation
    if [[ condition ]]; then
        log_success "Test passed"
    else
        log_error "Test failed"
        return 1
    fi
}

# Add to main() function
test_my_new_feature || true
```

### Troubleshooting

**Problem:** Tests fail with "Server is not accessible"
- **Solution:** Ensure Miao server is running on the configured port

**Problem:** Browser errors or timeouts
- **Solution:** Increase sleep durations or check agent-browser installation

**Problem:** Screenshots not generated
- **Solution:** Verify write permissions for screenshot directory

**Problem:** Login test fails
- **Solution:** Check that system is initialized with the correct password

### Best Practices

1. **Run tests before commits** - Verify functionality before pushing changes
2. **Review screenshots** - Visual verification of UI state during tests
3. **Update tests with features** - Add test cases for new functionality
4. **Keep tests idempotent** - Tests should be repeatable without manual cleanup
5. **Monitor test execution time** - Optimize slow tests with appropriate waits

### Test Maintenance

The UI test suite is designed to be self-documenting and easy to maintain:

- Each test function includes a descriptive log message
- Failed tests are captured with screenshots for debugging
- Test results include clear pass/fail indicators
- Configuration via environment variables allows flexibility

After running tests, review the generated report and screenshots to ensure all functionality works as expected. Update the test suite as new features are added to maintain comprehensive coverage.
