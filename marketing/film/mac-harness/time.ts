// The harness's clock, imported before anything else (harness.ts).
//
// ?clock=10:14  The page's local time of day (default 10:14, a weekday
//               morning), whatever time it is when filming. Time still runs.
//
// ?film=1       Slow-motion support for filming. Page time (Date,
//               performance.now, requestAnimationFrame, setTimeout,
//               setInterval) can be slowed with window.__harness.setSlow(k)
//               right before the camera rolls; capture.mjs slows CSS and Web
//               Animations to match through DevTools, acts k times slower,
//               and maps the recorded frames back to page time. The film then
//               plays at normal speed with k times more frames, even when the
//               Mac is too busy to screencast 4K at 30 fps. Native smooth
//               scrolling can't be slowed, so smooth scrollIntoView/scrollTo
//               are animated here instead (same feel as Chromium's).

const params = new URLSearchParams(location.search);

const RealDate = Date;
const realNow = RealDate.now.bind(RealDate);
const perfNow = performance.now.bind(performance);
const base = realNow();

/** Slow-motion factor (1 = real time) and where page time stood when it changed */
let k = 1;
let dateBase = { real: base, page: base };
let perfBase = { real: perfNow(), page: perfNow() };

const [hh, mm] = (params.get("clock") ?? "10:14").split(":").map(Number);
{
    const target = new RealDate(base);
    target.setHours(hh, mm, 0, 0);
    dateBase = { real: base, page: target.getTime() };
}

const pageDateNow = () => dateBase.page + (realNow() - dateBase.real) / k;
const pagePerfNow = () => perfBase.page + (perfNow() - perfBase.real) / k;

class HarnessDate extends RealDate {
    constructor(...args: any[]) {
        if (args.length === 0) super(pageDateNow());
        else super(...(args as [any]));
    }
    static now() {
        return pageDateNow();
    }
}
(window as any).Date = HarnessDate;

/** Slow page time down (k > 1) or back to real time (k = 1). Timers already
 *  scheduled keep the speed they were scheduled at. */
export function setSlow(next: number) {
    const now = realNow();
    const pnow = perfNow();
    dateBase = { real: now, page: pageDateNow() };
    perfBase = { real: pnow, page: pagePerfNow() };
    k = Math.max(1, next || 1);
}

if (params.get("film") === "1") {
    Object.defineProperty(performance, "now", { value: pagePerfNow, configurable: true });

    const raf = window.requestAnimationFrame.bind(window);
    window.requestAnimationFrame = (cb: FrameRequestCallback) => raf(() => cb(pagePerfNow()));

    const st = window.setTimeout.bind(window);
    window.setTimeout = ((fn: TimerHandler, ms?: number, ...a: unknown[]) => st(fn, (Number(ms) || 0) * k, ...a)) as typeof setTimeout;
    const si = window.setInterval.bind(window);
    window.setInterval = ((fn: TimerHandler, ms?: number, ...a: unknown[]) => si(fn, (Number(ms) || 0) * k, ...a)) as typeof setInterval;

    installSmoothScroll();
}

/** Smooth scrolling driven by (slowed) requestAnimationFrame. */
function installSmoothScroll() {
    const running = new WeakMap<Element, number>();

    function animate(el: Element, left: number, top: number) {
        const maxX = el.scrollWidth - el.clientWidth;
        const maxY = el.scrollHeight - el.clientHeight;
        left = Math.max(0, Math.min(maxX, left));
        top = Math.max(0, Math.min(maxY, top));
        const sx = el.scrollLeft;
        const sy = el.scrollTop;
        const dx = left - sx;
        const dy = top - sy;
        if (Math.abs(dx) < 1 && Math.abs(dy) < 1) return;
        const token = (running.get(el) ?? 0) + 1;
        running.set(el, token);
        const style = (el as HTMLElement).style;
        const before = style.scrollBehavior;
        style.scrollBehavior = "auto";
        // Close to Chromium's own: longer for longer distances, eased in and out
        const dur = Math.min(520, Math.max(260, Math.hypot(dx, dy) * 0.9));
        const t0 = performance.now();
        const step = () => {
            if (running.get(el) !== token) return;
            const t = Math.min(1, (performance.now() - t0) / dur);
            const e = t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2;
            el.scrollLeft = sx + dx * e;
            el.scrollTop = sy + dy * e;
            if (t < 1) requestAnimationFrame(step);
            else style.scrollBehavior = before;
        };
        requestAnimationFrame(step);
    }

    const scrollTo = Element.prototype.scrollTo;
    Element.prototype.scrollTo = function (this: Element, a?: ScrollToOptions | number, b?: number) {
        if (a && typeof a === "object" && a.behavior === "smooth") {
            animate(this, a.left ?? this.scrollLeft, a.top ?? this.scrollTop);
            return;
        }
        return (scrollTo as any).call(this, a, b);
    } as typeof Element.prototype.scrollTo;

    const scroller = (el: Element, axis: "x" | "y"): Element | null => {
        for (let p = el.parentElement; p; p = p.parentElement) {
            const cs = getComputedStyle(p);
            const ov = axis === "y" ? cs.overflowY : cs.overflowX;
            const room = axis === "y" ? p.scrollHeight > p.clientHeight : p.scrollWidth > p.clientWidth;
            if (room && /(auto|scroll)/.test(ov)) return p;
        }
        return null;
    };

    /** Where a container must scroll to show [a, b] (relative to its view) */
    const target = (pos: number, view: number, a: number, b: number, mode: string) => {
        if (mode === "center") return pos + (a + b) / 2 - view / 2;
        if (mode === "start") return pos + a;
        if (mode === "end") return pos + b - view;
        // nearest
        if (a < 0) return pos + a;
        if (b > view) return pos + Math.min(a, b - view);
        return pos;
    };

    const scrollIntoView = Element.prototype.scrollIntoView;
    Element.prototype.scrollIntoView = function (this: Element, arg?: boolean | ScrollIntoViewOptions) {
        if (!(arg && typeof arg === "object" && arg.behavior === "smooth")) return scrollIntoView.call(this, arg);
        const r = this.getBoundingClientRect();
        const ys = scroller(this, "y");
        const xs = scroller(this, "x");
        const along = (s: Element, axis: "x" | "y") => {
            const c = s.getBoundingClientRect();
            return axis === "y"
                ? target(s.scrollTop, s.clientHeight, r.top - c.top - s.clientTop, r.bottom - c.top - s.clientTop, arg.block ?? "start")
                : target(s.scrollLeft, s.clientWidth, r.left - c.left - s.clientLeft, r.right - c.left - s.clientLeft, arg.inline ?? "nearest");
        };
        if (ys && ys === xs) {
            animate(ys, along(ys, "x"), along(ys, "y"));
            return;
        }
        if (ys) animate(ys, ys.scrollLeft, along(ys, "y"));
        if (xs) animate(xs, along(xs, "x"), xs.scrollTop);
    } as typeof Element.prototype.scrollIntoView;
}
