// Applies the saved light or dark theme before the first paint. The content security policy
// allows only scripts from the manager itself, so this cannot be an inline script.
try {
  const theme = localStorage.getItem("thoric-theme") || "dark"
  const dark = theme === "dark" || (theme === "system" && matchMedia("(prefers-color-scheme: dark)").matches)
  document.documentElement.classList.toggle("dark", dark)
  document.documentElement.style.colorScheme = dark ? "dark" : "light"
} catch {
  // Without storage the page keeps the dark default.
}
