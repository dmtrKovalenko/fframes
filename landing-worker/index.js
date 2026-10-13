// A/B test of the landing page intro video. Only the page itself runs through this Worker
// first (`assets.run_worker_first` in wrangler.jsonc); videos, posters and the rest of
// landing/ are served as plain static assets.
//
// index.html is variant A as is. A visitor gets a variant at random on the first visit and
// keeps it through the `intro` cookie; `?intro=a` or `?intro=b` shows a variant for that
// request without changing the cookie. The variant goes to Plausible as the `intro`
// property of the pageview (`event-intro` on the script tag) and of the custom events sent
// from index.html, which read it from `<html data-intro>`.

const COOKIE = "intro";
const COOKIE_MAX_AGE = 60 * 60 * 24 * 90;

/** Attribute and text replacements per variant, applied to index.html. */
const VARIANTS = {
  a: null,
  b: {
    poster: "made-of-motion-poster.jpg",
    av1: "made-of-motion.av1.mp4",
    h264: "made-of-motion.mp4",
    label: "Made of motion, a film rebuilt with fframes",
    caption:
      'Jordan Watkins’ film, rebuilt in Rust with <strong>fframes</strong> and <a href="https://github.com/dmtrKovalenko/fframes/tree/main/examples/made-of-motion">open source</a>',
  },
};

const isVariant = (value) => Object.hasOwn(VARIANTS, value);

function chooseVariant(request, url) {
  const forced = url.searchParams.get(COOKIE);
  if (isVariant(forced)) return { intro: forced, persist: false };

  const cookie = request.headers.get("Cookie") ?? "";
  const stored = new RegExp(`(?:^|;\\s*)${COOKIE}=([a-z])`).exec(cookie)?.[1];
  if (isVariant(stored)) return { intro: stored, persist: false };

  const keys = Object.keys(VARIANTS);
  return { intro: keys[Math.floor(Math.random() * keys.length)], persist: true };
}

function rewrite(response, intro) {
  const variant = VARIANTS[intro];
  let rewriter = new HTMLRewriter()
    .on("html", { element: (el) => el.setAttribute("data-intro", intro) })
    .on("script[data-domain]", { element: (el) => el.setAttribute("event-intro", intro) });

  if (variant) {
    rewriter = rewriter
      .on("figure video", {
        element(el) {
          el.setAttribute("poster", variant.poster);
          el.setAttribute("aria-label", variant.label);
        },
      })
      .on('figure source[data-codec="av1"]', { element: (el) => el.setAttribute("src", variant.av1) })
      .on('figure source[data-codec="h264"]', { element: (el) => el.setAttribute("src", variant.h264) })
      .on("figcaption span", { element: (el) => el.setInnerContent(variant.caption, { html: true }) });
  }

  return rewriter.transform(response);
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    // Paths that match no asset reach the Worker too; leave their 404 to the assets.
    const isPage = url.pathname === "/" || url.pathname === "/index.html";
    if (!isPage || (request.method !== "GET" && request.method !== "HEAD")) {
      return env.ASSETS.fetch(request);
    }

    // Ask for the page without the browser's conditional headers: a 304 would let the
    // browser reuse a cached page of the other variant.
    const page = await env.ASSETS.fetch(new Request(new URL("/", url), { method: request.method }));
    if (!page.ok || !page.headers.get("Content-Type")?.startsWith("text/html")) return page;

    const { intro, persist } = chooseVariant(request, url);
    const response = new Response(rewrite(page, intro).body, page);
    response.headers.delete("ETag");
    response.headers.set("Cache-Control", "private, no-cache");
    response.headers.append("Vary", "Cookie");
    if (persist) {
      response.headers.append(
        "Set-Cookie",
        `${COOKIE}=${intro}; Path=/; Max-Age=${COOKIE_MAX_AGE}; SameSite=Lax; Secure`,
      );
    }
    return response;
  },
};
