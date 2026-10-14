import * as React from "react"

import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Field, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Spinner } from "@/components/ui/spinner"
import { api } from "@/lib/api"
import { queryClient, signedIn } from "@/lib/queries"

export function LoginPage() {
  const [username, setUsername] = React.useState("")
  const [password, setPassword] = React.useState("")
  const [busy, setBusy] = React.useState(false)
  const [error, setError] = React.useState("")
  const id = React.useId()

  const submit = async (event: React.FormEvent) => {
    event.preventDefault()
    setBusy(true)
    setError("")
    try {
      await api("login", { username, password })
      signedIn()
      await queryClient.invalidateQueries({ queryKey: ["session"] })
    } catch (failure) {
      setError((failure as Error).message)
      setBusy(false)
    }
  }

  return (
    <main className="flex min-h-svh flex-col items-center justify-center gap-6 bg-sidebar p-6 md:p-10">
      <div className="flex w-full max-w-sm flex-col gap-6">
        <a
          href="/"
          className="flex items-center gap-3 self-center text-lg font-semibold tracking-wide"
        >
          <img src="/mark.svg" alt="" className="size-10" />
          Bilskirnir
        </a>
        <Card>
          <CardHeader className="text-center">
            <CardTitle className="text-xl">Sign in</CardTitle>
            <CardDescription>Use the administrator account of this installation.</CardDescription>
          </CardHeader>
          <CardContent>
            <form onSubmit={submit}>
              <FieldGroup>
                <Field>
                  <FieldLabel htmlFor={`${id}-username`}>Username</FieldLabel>
                  <Input
                    id={`${id}-username`}
                    autoComplete="username"
                    required
                    autoFocus
                    value={username}
                    onChange={(event) => setUsername(event.target.value)}
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor={`${id}-password`}>Password</FieldLabel>
                  <Input
                    id={`${id}-password`}
                    type="password"
                    autoComplete="current-password"
                    required
                    value={password}
                    onChange={(event) => setPassword(event.target.value)}
                  />
                </Field>
                <FieldError>{error}</FieldError>
                <Button type="submit" disabled={busy}>
                  {busy && <Spinner data-icon="inline-start" />}
                  Sign in
                </Button>
              </FieldGroup>
            </form>
          </CardContent>
        </Card>
      </div>
    </main>
  )
}
