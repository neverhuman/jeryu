// DeleteRepoDialog.test.tsx — confirmation gating for the two removal tiers.
//
// Purge tier: the confirm button must stay disabled until the user types
// the exact `owner/name` (byte match — close misses do not count). Registry
// tier: no typed confirmation, but `busy` still disables the button. Error
// messages render with role="alert".

import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { DeleteRepoDialog } from '../DeleteRepoDialog';

describe('DeleteRepoDialog', () => {
  it('renders nothing when closed', () => {
    const { container } = render(
      <DeleteRepoDialog
        open={false}
        tier="purge"
        fullName="veox/redline"
        onConfirm={() => {}}
        onCancel={() => {}}
      />
    );
    expect(container).toBeEmptyDOMElement();
  });

  it('purge: confirm stays disabled until the exact repo name is typed', async () => {
    const user = userEvent.setup();
    const onConfirm = vi.fn();
    render(
      <DeleteRepoDialog
        open
        tier="purge"
        fullName="veox/redline"
        confirmLabel="Purge repository and storage"
        onConfirm={onConfirm}
        onCancel={() => {}}
      />
    );

    const confirm = screen.getByRole('button', {
      name: 'Purge repository and storage',
    });
    expect(confirm).toBeDisabled();
    // The destructive copy is explicit.
    expect(screen.getByRole('dialog')).toHaveTextContent('cannot be undone');

    const input = screen.getByLabelText(/to confirm/);
    await user.type(input, 'veox/redlin');
    expect(confirm).toBeDisabled();

    // A near miss (case difference) must not unlock the button.
    await user.clear(input);
    await user.type(input, 'VEOX/redline');
    expect(confirm).toBeDisabled();

    await user.clear(input);
    await user.type(input, 'veox/redline');
    expect(confirm).toBeEnabled();

    await user.click(confirm);
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it('registry: confirm is enabled without typing and the copy says storage is kept', () => {
    render(
      <DeleteRepoDialog
        open
        tier="registry"
        fullName="veox/redline"
        confirmLabel="Remove from registry"
        onConfirm={() => {}}
        onCancel={() => {}}
      />
    );
    expect(
      screen.getByRole('button', { name: 'Remove from registry' })
    ).toBeEnabled();
    expect(screen.getByRole('dialog')).toHaveTextContent(
      'bare storage on disk is kept'
    );
    expect(screen.queryByLabelText(/to confirm/)).not.toBeInTheDocument();
  });

  it('disables confirm while the mutation is in flight', () => {
    render(
      <DeleteRepoDialog
        open
        tier="registry"
        fullName="veox/redline"
        busy
        onConfirm={() => {}}
        onCancel={() => {}}
      />
    );
    expect(screen.getByRole('button', { name: 'Remove' })).toBeDisabled();
  });

  it('surfaces the error message with role="alert"', () => {
    render(
      <DeleteRepoDialog
        open
        tier="purge"
        fullName="veox/redline"
        errorMessage="confirm_full_name does not match the repository."
        onConfirm={() => {}}
        onCancel={() => {}}
      />
    );
    expect(screen.getByRole('alert')).toHaveTextContent(
      'confirm_full_name does not match the repository.'
    );
  });

  it('shows the critical risk badge on purge and high on registry', () => {
    const { rerender } = render(
      <DeleteRepoDialog
        open
        tier="purge"
        fullName="veox/redline"
        onConfirm={() => {}}
        onCancel={() => {}}
      />
    );
    expect(screen.getByLabelText('Risk: Critical')).toBeInTheDocument();
    rerender(
      <DeleteRepoDialog
        open
        tier="registry"
        fullName="veox/redline"
        onConfirm={() => {}}
        onCancel={() => {}}
      />
    );
    expect(screen.getByLabelText('Risk: High')).toBeInTheDocument();
  });
});
