/** Summary of a community armor profile. */
export interface ProfileSummary {
  /** Profile name (e.g. "github"). */
  name: string;
  /** Filename in profiles/community/. */
  filename: string;
  /** One-line description of what it allows. */
  description: string;
}

/** Community profiles bundled in v1. */
export const COMMUNITY_PROFILES: ProfileSummary[] = [
  {
    name: "github",
    filename: "github.armor.json",
    description: "Outbound HTTPS to api.github.com and github.com only; env: GITHUB_TOKEN",
  },
  {
    name: "filesystem",
    filename: "filesystem.armor.json",
    description: "Read/write to /tmp/mcparmor/* only; no network; no spawn",
  },
  {
    name: "fetch",
    filename: "fetch.armor.json",
    description: "Outbound HTTP/HTTPS to any host (*:443, *:80); no filesystem; no spawn",
  },
  {
    name: "git",
    filename: "git.armor.json",
    description: "Read/write entire filesystem; spawn allowed (git forks); no network",
  },
  {
    name: "sqlite",
    filename: "sqlite.armor.json",
    description: "Read/write .db and .sqlite under /tmp/mcparmor/ only; no network; no spawn",
  },
  {
    name: "brave-search",
    filename: "brave-search.armor.json",
    description: "Outbound HTTPS to api.search.brave.com only; env: BRAVE_API_KEY",
  },
  {
    name: "slack",
    filename: "slack.armor.json",
    description: "Outbound HTTPS to slack.com, api.slack.com, files.slack.com; env: SLACK_BOT_TOKEN",
  },
  {
    name: "notion",
    filename: "notion.armor.json",
    description: "Outbound HTTPS to api.notion.com only; env: NOTION_TOKEN",
  },
  {
    name: "playwright",
    filename: "playwright.armor.json",
    description: "Outbound HTTP/HTTPS to any host; read Playwright config; spawn allowed",
  },
  {
    name: "gmail",
    filename: "gmail.armor.json",
    description: "Outbound HTTPS to Gmail and Google OAuth endpoints; env: OAuth credentials",
  },
];
