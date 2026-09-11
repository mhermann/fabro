export const INSTALL_PROVIDERS = [
  {
    id: "anthropic",
    label: "Anthropic",
    envVar: "ANTHROPIC_API_KEY",
    keyHelp: {
      url: "https://console.anthropic.com/settings/keys",
      text: "Create one in the Anthropic Console under Settings → API keys.",
    },
  },
  {
    id: "openai",
    label: "OpenAI",
    envVar: "OPENAI_API_KEY",
    keyHelp: {
      url: "https://platform.openai.com/api-keys",
      text: "Create one in the OpenAI platform under API keys.",
    },
  },
  {
    id: "gemini",
    label: "Gemini",
    envVar: "GEMINI_API_KEY",
    keyHelp: {
      url: "https://aistudio.google.com/apikey",
      text: "Create one in Google AI Studio.",
    },
  },
] as const;

export const FORGEJO_TOKEN_HELP = {
  url: "https://docs.forgejo.org/latest/user/authentication/access-token/",
  text: "Create an access token under Settings → Applications → Access tokens with repository read and write scope.",
} as const;
