THE WHY — landing page

Run locally:
  cd landing
  npm run dev
Then visit http://127.0.0.1:4173.
No dependency installation or build step is required.

Deploy index.html, style.css, app.js, _headers, vendor/ and assets/ to a
static host, or deploy the landing directory directly to Cloudflare Pages.
All files are below the 25 MiB per-file limit. No progressive movie is included.
The preview server and _headers provide the HLS and WebVTT MIME types.

Streaming:
  assets/hls/master.m3u8 selects 1080p or 720p automatically.
  Each rendition has 25 fMP4 segments: 24 x 4 seconds and a 2-second ending.
  H.264 High / AAC stereo, 60 fps. Each rendition has its own initialization MP4.
  Browsers with native HLS play it directly; other MSE-capable browsers use
  locally vendored hls.js 1.6.13 (Apache-2.0, see vendor/hls.LICENSE.txt),
  which loads only when the player nears the viewport or a play button is used.
  Video segments load on user playback, not on page load.
  English narration captions and scene buttons work with either player.

Regenerate both renditions from the original 4K master:
  sh encode-hls.sh
Requires FFmpeg with libx264 and AAC. The script expects ../why.mp4 and
replaces the generated HLS outputs. The original master remains unchanged.

All page assets are local; no CDN or external fonts are required.

Analytics: Plausible (domain fframes.studio, same site as the homepage).
The script records the pageview, scroll depth and time on page. app.js sends:
  Film play        {from: hero|player}
  Film progress    {percent: 25|50|75}, share of the film actually played
  Film finished    the film reached its end
  Film replay      Watch again on the end screen
  Film error       {reason}
  Scene play       {scene}
  Fact preview     {fact}, on click or tap, once per fact
  Share            {method: native|copy link, location}
  Copy command     {command: skill|cargo}
  GitHub click     {location, path}, the same goal name as the homepage
  Outbound click   {location, host}
Add each name as a custom event goal in Plausible to see it in the dashboard.
Still images were extracted from the movie. Narration captions follow
SCRIPT.md timing. Film facts are based on SCRIPT.md, src/lib.rs and ASSETS.md.
The code panel is illustrative, not an executable standalone program.
Decorative animations follow prefers-reduced-motion; the header has no motion toggle.

Icons: official Lucide SVGs, lucide-static 0.468.0, inline in index.html.
Original SVGs are retained in assets/icons; license: vendor/lucide.LICENSE.txt.

NYC fact provenance (measured from local assets):
  dynamic_media/nyc/DA_WISE_GML.zip: 916,369,666 compressed bytes.
  Expanded archive: 13,782,417,719 bytes (13.8 decimal GB).
  dynamic_media/nyc/manhattan.bin: 57,533,943 bytes (57.5 decimal MB).
The processed asset includes a selected NYC region plus other city data;
these numbers are not a lossless compression ratio for the full NYC model.

Render-time fact: 5,880 frames in 94 seconds, supplied by the project owner.
Billboard fact: make-billboards.sh builds a 20-tile atlas for Times Square;
Made of Motion appears on tile 14 and on tiles without other cameos.
Jordan Watkins credit: examples/made-of-motion/README.md identifies the work
as an adaptation and links to the original film at
https://x.com/jordanarchivess/status/2107536262894915762.

Fact rows switch the left preview on hover, keyboard focus or tap.
Scene cards play HLS inline from their scene start; Close or Escape restores
the still and returns focus. Only one player plays at a time. Closing either
player releases its HLS buffers and network activity.

Production route: https://fframes.studio/the-why
Run npm run build:landing from the repository root to stage this page and its
assets in landing/the-why. That generated directory is ignored by Git; keep
this source directory and its HLS media in the repository for deployment.
The build injects /the-why/ as the base URL so both route forms resolve assets.
Root landing/_headers supplies the production HLS content types.
Run npm run preview:landing from the repository root for the combined site
at http://127.0.0.1:4174, including the homepage premiere link.
