import * as React from "react"
import { FolderOpenIcon, PlusIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Spinner } from "@/components/ui/spinner"
import { projectNameError } from "@/lib/project-name"
import * as m from "@/paraglide/messages.js"

export function CreateProjectDialog({
  open,
  busy,
  name,
  parent,
  error,
  onOpenChange,
  onNameChange,
  onChooseParent,
  onSubmit,
}: {
  open: boolean
  busy: boolean
  name: string
  parent: string
  error: string | null
  onOpenChange: (open: boolean) => void
  onNameChange: (value: string) => void
  onChooseParent: () => void
  onSubmit: (event: React.FormEvent) => void
}) {
  const invalidName = Boolean(name && projectNameError(name))

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <form className="flex flex-col gap-6" onSubmit={onSubmit}>
          <DialogHeader>
            <DialogTitle>{m.create_title()}</DialogTitle>
            <DialogDescription>{m.create_description()}</DialogDescription>
          </DialogHeader>
          <FieldGroup>
            <Field data-invalid={invalidName || undefined}>
              <FieldLabel htmlFor="project-name">
                {m.field_project_name()}
              </FieldLabel>
              <Input
                id="project-name"
                value={name}
                autoFocus
                aria-invalid={invalidName || undefined}
                placeholder={m.create_name_placeholder()}
                onChange={(event) => onNameChange(event.target.value)}
              />
              <FieldDescription>{m.create_name_description()}</FieldDescription>
            </Field>
            <Field data-invalid={!parent && Boolean(error) ? true : undefined}>
              <FieldLabel>{m.field_project_location()}</FieldLabel>
              <Button
                type="button"
                variant="outline"
                onClick={onChooseParent}
                disabled={busy}
              >
                <FolderOpenIcon data-icon="inline-start" />
                {parent ? m.create_change_folder() : m.common_choose_folder()}
              </Button>
              <FieldDescription>
                {parent || m.create_no_folder()}
              </FieldDescription>
            </Field>
            <FieldError>{error}</FieldError>
          </FieldGroup>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
              disabled={busy}
            >
              {m.common_cancel()}
            </Button>
            <Button type="submit" disabled={busy}>
              {busy ? (
                <Spinner data-icon="inline-start" />
              ) : (
                <PlusIcon data-icon="inline-start" />
              )}
              {m.create_project()}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
