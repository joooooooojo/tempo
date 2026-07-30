import { defineConfig } from "vitepress";

export default defineConfig({
  title: "Tempo 文档",
  description: "Tempo 使用指南、插件开发教程与 API 参考",
  lang: "zh-CN",
  base: "/tempo/",
  cleanUrls: true,
  lastUpdated: true,
  srcExclude: ["design/**/*.md"],
  head: [
    ["link", { rel: "icon", href: "/tempo/favicon.png" }],
    ["meta", { name: "theme-color", content: "#f7f9f6" }],
  ],
  markdown: {
    lineNumbers: true,
  },
  themeConfig: {
    logo: "/favicon.png",
    siteTitle: "Tempo 文档",
    nav: [
      { text: "使用指南", link: "/guide/getting-started", activeMatch: "/guide/" },
      { text: "插件开发", link: "/developer/", activeMatch: "/developer/" },
      { text: "API 参考", link: "/reference/", activeMatch: "/reference/" },
      {
        text: "下载",
        link: "https://github.com/joooooooojo/tempo/releases",
      },
    ],
    sidebar: {
      "/guide/": [
        {
          text: "使用指南",
          items: [
            { text: "快速开始", link: "/guide/getting-started" },
            { text: "日常使用", link: "/guide/daily-use" },
            { text: "安装与管理插件", link: "/guide/plugins" },
            { text: "遇到问题", link: "/guide/troubleshooting" },
          ],
        },
      ],
      "/developer/": [
        {
          text: "插件开发",
          items: [
            { text: "从这里开始", link: "/developer/" },
            { text: "做出第一个插件", link: "/developer/first-plugin" },
            { text: "插件类型与生命周期", link: "/developer/plugin-lifecycle" },
            { text: "加入后台能力", link: "/developer/runtime" },
          ],
        },
        {
          text: "继续查阅",
          items: [
            { text: "API 参考", link: "/reference/" },
            { text: "完整示例", link: "https://github.com/joooooooojo/tempo/tree/master/examples/plugins/com.example.hello" },
          ],
        },
      ],
      "/reference/": [
        {
          text: "API 参考",
          items: [
            { text: "如何查阅", link: "/reference/" },
            { text: "插件全局 API", link: "/reference/plugin-api" },
            { text: "平台 API", link: "/reference/plugin-host-api" },
            { text: "宿主事件", link: "/reference/host-events" },
            { text: "Manifest", link: "/reference/manifest-schema" },
          ],
        },
      ],
    },
    socialLinks: [
      { icon: "github", link: "https://github.com/joooooooojo/tempo" },
    ],
    search: {
      provider: "local",
    },
    outline: {
      label: "本页目录",
      level: [2, 3],
    },
    docFooter: {
      prev: "上一页",
      next: "下一页",
    },
    sidebarMenuLabel: "文档目录",
    returnToTopLabel: "回到顶部",
    darkModeSwitchLabel: "外观",
    lastUpdated: {
      text: "最后更新",
    },
    editLink: {
      pattern:
        "https://github.com/joooooooojo/tempo/edit/master/docs/:path",
      text: "在 GitHub 上编辑此页",
    },
    footer: {
      message: "Tempo 在本机运行，你的数据默认留在本机。",
      copyright: "Copyright © Tempo contributors",
    },
  },
});
