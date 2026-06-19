import { viteBundler } from "@vuepress/bundler-vite";
import { defaultTheme } from "@vuepress/theme-default";
import { defineUserConfig } from "vuepress";

export default defineUserConfig({
  lang: "zh-CN",
  title: "Nalu 文档",
  description: "Nalu 本地优先个人助手的产品、开发和运维文档",
  bundler: viteBundler(),
  theme: defaultTheme({
    logo: null,
    repo: "nalomu-uni-platform",
    docsDir: "docs",
    navbar: [
      { text: "指南", link: "/guide/" },
      { text: "功能", link: "/features/" },
      { text: "开发", link: "/development/" },
      { text: "运维", link: "/operations/" },
      { text: "归档", link: "/archive/" },
    ],
    sidebar: {
      "/guide/": [
        "/guide/",
        "/guide/getting-started.md",
        "/guide/mobile-testing.md",
      ],
      "/features/": [
        "/features/",
        "/features/alerts-and-sounds.md",
        "/features/dashboard.md",
      ],
      "/development/": [
        "/development/",
        "/development/architecture-map.md",
        "/development/commands.md",
      ],
      "/operations/": [
        "/operations/",
        "/operations/android-build.md",
        "/operations/cache-relocation.md",
      ],
      "/archive/": [
        "/archive/",
        "/ARCHITECTURE.md",
        "/TECH_STACK.md",
        "/IMPLEMENTATION.md",
        "/PRD-本地助手软件基座.md",
        "/POSTMORTEM-multi-webview-audio.md",
      ],
    },
  }),
});
