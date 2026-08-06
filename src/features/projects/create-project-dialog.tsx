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
            <DialogTitle>Create a new project</DialogTitle>
            <DialogDescription>
              A .rosettacue package keeps source metadata, cue images, OCR, and
              edits together.
            </DialogDescription>
          </DialogHeader>
          <FieldGroup>
            <Field data-invalid={invalidName || undefined}>
              <FieldLabel htmlFor="project-name">Project name</FieldLabel>
              <Input
                id="project-name"
                value={name}
                autoFocus
                aria-invalid={invalidName || undefined}
                placeholder="Film title"
                onChange={(event) => onNameChange(event.target.value)}
              />
              <FieldDescription>
                Use the film or disc name. The package uses the same name.
              </FieldDescription>
            </Field>
            <Field data-invalid={!parent && Boolean(error) ? true : undefined}>
              <FieldLabel>Project location</FieldLabel>
              <Button
                type="button"
                variant="outline"
                onClick={onChooseParent}
                disabled={busy}
              >
                <FolderOpenIcon data-icon="inline-start" />
                {parent ? "Change folder" : "Choose folder"}
              </Button>
              <FieldDescription>
                {parent || "No folder selected"}
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
              Cancel
            </Button>
            <Button type="submit" disabled={busy}>
              {busy ? (
                <Spinner data-icon="inline-start" />
              ) : (
                <PlusIcon data-icon="inline-start" />
              )}
              Create project
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
