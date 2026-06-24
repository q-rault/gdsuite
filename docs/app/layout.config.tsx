import type { BaseLayoutProps } from "fumadocs-ui/layouts/shared";
import { GitHubIcon } from "@/components/github-icon";

export const baseOptions: BaseLayoutProps = {
  nav: {
    title: "GDSuite Docs",
  },
  links: [
    {
      type: "icon",
      text: "GitHub",
      label: "GitHub",
      url: "https://github.com/q-rault/gdsuite",
      external: true,
      icon: <GitHubIcon />,
    },
  ],
};
