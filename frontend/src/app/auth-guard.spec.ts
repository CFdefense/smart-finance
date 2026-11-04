import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TestBed } from '@angular/core/testing';
import { Router, UrlTree } from '@angular/router';
import { firstValueFrom, from, isObservable, of, throwError } from 'rxjs';

import { authGuard } from './auth-guard';
import { AuthService } from './services/auth.service';

describe('authGuard', () => {
  let authService: {
    checkAuth: ReturnType<typeof vi.fn>;
  };
  let router: Router;

  beforeEach(() => {
    authService = {
      checkAuth: vi.fn().mockReturnValue(of(void 0)),
    };

    TestBed.configureTestingModule({
      providers: [
        { provide: AuthService, useValue: authService },
        { provide: Router, useClass: Router },
      ],
    });

    router = TestBed.inject(Router);
  });

  function runGuard() {
    const result = TestBed.runInInjectionContext(() => authGuard({} as never, {} as never));

    return isObservable(result) ? result : from(Promise.resolve(result));
  }

  /**
   * Checks that the route guard permits authenticated users.
   * Runs the guard with a successful mocked auth check and expects true.
   */
  it('should allow authenticated users', async () => {
    const result = await firstValueFrom(runGuard());

    expect(authService.checkAuth).toHaveBeenCalled();
    expect(result).toBe(true);
  });

  /**
   * Checks that unauthenticated users cannot access a protected route.
   * Mocks a 401 auth failure and verifies the guard returns a /login UrlTree.
   */
  it('should redirect unauthenticated users to login', async () => {
    authService.checkAuth.mockReturnValue(throwError(() => ({ status: 401 })));

    const result = await firstValueFrom(runGuard());

    expect(result).toBeInstanceOf(UrlTree);
    expect(router.serializeUrl(result as UrlTree)).toBe('/login');
  });
});
