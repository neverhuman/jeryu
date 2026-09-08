import { LogIn, UserPlus } from 'lucide-react';
import { useEffect, useState, type FormEvent, type Ref } from 'react';
import { useNavigate } from 'react-router-dom';

import { ApiError } from '../api/client';
import { ActionButton } from '../components/action/ActionButton';
import { JeryuLogo } from '../components/brand/JeryuLogo';
import { useAuth } from '../hooks/useAuth';

import './AuthPage.css';

type Mode = 'login' | 'signup';

/**
 * AuthForm — the tabs + credential form shared by the boot LoginPanel and the
 * forced-password-change page. This component owns the accessibility contract
 * exercised by the e2e suite (tab roles `Login`/`Sign up`, labeled `Username`/
 * `Password`/`New password` inputs, submit buttons `Login`/`Create account`/
 * `Change password`) — keep those names/labels stable.
 */
export function AuthForm({
  forcePasswordChange = false,
  initialMode = 'login',
  firstFieldRef,
}: {
  forcePasswordChange?: boolean;
  initialMode?: Mode;
  firstFieldRef?: Ref<HTMLInputElement>;
}): JSX.Element {
  const auth = useAuth();
  const navigate = useNavigate();
  const [mode, setMode] = useState<Mode>(initialMode);
  const [login, setLogin] = useState('');
  const [password, setPassword] = useState('');
  const [newPassword, setNewPassword] = useState('');
  const [rememberMe, setRememberMe] = useState(false);
  const active = mode === 'login' ? auth.login : auth.signup;
  const error = forcePasswordChange ? auth.changePassword.error : active.error;

  useEffect(() => {
    if (!forcePasswordChange) {
      setMode(initialMode);
    }
  }, [forcePasswordChange, initialMode]);

  const submit = async (event: FormEvent): Promise<void> => {
    event.preventDefault();
    if (forcePasswordChange) {
      const user = await auth.changePassword.mutateAsync({
        currentPassword: password,
        newPassword,
      });
      if (user && !user.mustChangePassword) {
        navigate('/repos/family/jeryu-split', { replace: true });
      }
      return;
    }
    const user =
      mode === 'login'
        ? await auth.login.mutateAsync({ login, password, rememberMe })
        : await auth.signup.mutateAsync({ login, password });
    if (user && !user.mustChangePassword) {
      navigate('/repos/family/jeryu-split', { replace: true });
    }
  };

  return (
    <>
      {forcePasswordChange ? null : (
        <div className="auth-panel__tabs" role="tablist">
          <button
            type="button"
            role="tab"
            aria-selected={mode === 'login'}
            className="auth-panel__tab"
            onClick={() => setMode('login')}
          >
            <LogIn size={14} aria-hidden="true" />
            Login
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={mode === 'signup'}
            className="auth-panel__tab"
            onClick={() => {
              setMode('signup');
              setRememberMe(false);
            }}
          >
            <UserPlus size={14} aria-hidden="true" />
            Sign up
          </button>
        </div>
      )}
      <form className="auth-form" onSubmit={(event) => void submit(event)}>
        {forcePasswordChange ? null : (
          <label className="auth-form__field">
            <span>Username</span>
            <input
              ref={firstFieldRef}
              value={login}
              onChange={(event) => setLogin(event.currentTarget.value)}
              autoComplete="username"
              required
            />
          </label>
        )}
        <label className="auth-form__field">
          <span>{forcePasswordChange ? 'Current password' : 'Password'}</span>
          <input
            ref={forcePasswordChange ? firstFieldRef : undefined}
            value={password}
            onChange={(event) => setPassword(event.currentTarget.value)}
            type="password"
            autoComplete={mode === 'login' ? 'current-password' : 'new-password'}
            minLength={12}
            required
          />
        </label>
        {forcePasswordChange ? (
          <label className="auth-form__field">
            <span>New password</span>
            <input
              value={newPassword}
              onChange={(event) => setNewPassword(event.currentTarget.value)}
              type="password"
              autoComplete="new-password"
              minLength={12}
              required
            />
          </label>
        ) : null}
        {!forcePasswordChange && mode === 'login' ? (
          <label className="auth-form__check">
            <input
              type="checkbox"
              checked={rememberMe}
              onChange={(event) => setRememberMe(event.currentTarget.checked)}
            />
            <span>Remember me</span>
          </label>
        ) : null}
        {error ? (
          <p className="auth-form__error">
            {error instanceof ApiError ? error.message : 'Authentication failed.'}
          </p>
        ) : null}
        <ActionButton
          type="submit"
          variant="primary"
          icon={mode === 'login' ? <LogIn size={14} /> : <UserPlus size={14} />}
          disabled={forcePasswordChange ? auth.changePassword.isPending : active.isPending}
        >
          {forcePasswordChange
            ? 'Change password'
            : mode === 'login'
              ? 'Login'
              : 'Create account'}
        </ActionButton>
      </form>
    </>
  );
}

/**
 * AuthPage — centered account surface used for the forced-password-change flow
 * (and as a standalone fallback). The signed-out splash/login experience is
 * the boot `LoginPanel`; both render an `<h1>JeRyu</h1>` heading + `AuthForm`.
 */
export function AuthPage({
  forcePasswordChange = false,
  initialMode = 'login',
}: {
  forcePasswordChange?: boolean;
  initialMode?: Mode;
}): JSX.Element {
  return (
    <main className="auth-page">
      <section className="auth-panel" aria-label="JeRyu account access">
        <div className="auth-panel__brand">
          <JeryuLogo variant="mark" className="auth-panel__mark" decorative />
          <h1 className="auth-panel__title">JeRyu</h1>
        </div>
        <AuthForm
          forcePasswordChange={forcePasswordChange}
          initialMode={initialMode}
        />
      </section>
    </main>
  );
}
