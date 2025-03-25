import reactRefresh from "@vitejs/plugin-react";
import { defineConfig } from "vite";
import { VitePWA } from "vite-plugin-pwa";

export default defineConfig({
  assetsInclude: ["./media/*", "fframes-editor/*.wasm"],
  server: {
    fs: {
      strict: false,
    },
  },
  optimizeDeps: {
    entries: ["./main.tsx"],
  },
  plugins: [
    reactRefresh(),
    VitePWA({
      devOptions: {
        enabled: true,
        type: "module",
        navigateFallback: "index.html",
      },
      registerType: "autoUpdate",
      mode: "production",
      workbox: {
        skipWaiting: true,
        runtimeCaching: [
          {
            urlPattern: ({ url }) => {
              const fileExtension = url.pathname
                .split(".")
                .pop()
                ?.toLowerCase();
              return [
                "mp3",
                "mp4",
                "wav",
                "ogg",
                "webm",
                "m4a",
                "flac",
                "aac", // Audio
                "webp",
                "avif",
                "mov",
                "avi",
                "mkv", // Video
                "woff",
                "woff2",
                "ttf",
                "otf",
                "eot", // Fonts
                "png",
                "jpeg",
                "jpg",
                "gif",
                "svg", // Images
              ].includes(fileExtension || "");
            },
            handler: "CacheFirst",
            options: {
              cacheName: "media-assets-cache",
              expiration: {
                maxEntries: 10_000,
                maxAgeSeconds: 60 * 60 * 24 * 30, // 30 days
              },
              cacheableResponse: {
                statuses: [0, 200],
              },
            },
          },
        ],
      },
    }),
  ],
});
