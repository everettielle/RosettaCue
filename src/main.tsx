import { StrictMode } from "react"
import { createRoot } from "react-dom/client"

import "./index.css"
import App from "./App.tsx"
import { ThemeProvider } from "@/components/theme-provider.tsx"
import { DebugLogWindow } from "@/features/settings/debug-log-window.tsx"
import { getLocale, getTextDirection } from "@/paraglide/runtime.js"

document.documentElement.lang = getLocale()
document.documentElement.dir = getTextDirection()

const view = new URLSearchParams(window.location.search).get("view")

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ThemeProvider>
      {view === "diagnostics" ? <DebugLogWindow /> : <App />}
    </ThemeProvider>
  </StrictMode>
)
