import { Toaster as Sonner } from 'sonner';

type ToasterProps = React.ComponentProps<typeof Sonner>;

function Toaster({ ...props }: ToasterProps) {
  return (
    <Sonner
      className="toaster group"
      toastOptions={{
        classNames: {
          toast:
            'group toast group-[.toaster]:bg-[var(--color-bg-secondary)] group-[.toaster]:text-[var(--color-text-primary)] group-[.toaster]:border-[var(--color-border-default)] group-[.toaster]:shadow-lg',
          description: 'group-[.toast]:text-[var(--color-text-secondary)]',
          actionButton:
            'group-[.toast]:bg-[var(--color-bg-secondary)] group-[.toast]:text-[var(--color-text-primary)]',
          cancelButton:
            'group-[.toast]:bg-[var(--color-bg-tertiary)] group-[.toast]:text-[var(--color-text-secondary)]',
        },
      }}
      {...props}
    />
  );
}

export { Toaster };
