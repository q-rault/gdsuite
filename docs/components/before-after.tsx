import { Children, isValidElement, type ReactNode } from 'react';

/**
 * Two panes side by side (stacked on narrow screens), for showing what a
 * feature changes: a script before and after formatting, a problem and its fix.
 */
export function BeforeAfter({
  before = 'Before',
  after = 'After',
  children,
}: {
  before?: string;
  after?: string;
  children: ReactNode;
}) {
  const panes = Children.toArray(children).filter(isValidElement);
  return (
    <div className="my-6 grid gap-4 md:grid-cols-2">
      {[before, after].map((label, index) => (
        <figure key={label} className="m-0 min-w-0">
          <figcaption className="mb-1.5 text-sm font-medium text-fd-muted-foreground">{label}</figcaption>
          <div className="min-w-0 [&_figure]:my-0">{panes[index]}</div>
        </figure>
      ))}
    </div>
  );
}
