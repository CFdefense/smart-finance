import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TestBed } from '@angular/core/testing';
import { Router } from '@angular/router';
import { of, throwError } from 'rxjs';

import { UserDashboard } from './dashboard';
import { AuthService } from '../app/services/auth.service';

describe('UserDashboard', () => {
  let component: UserDashboard;
  let authService: {
    logout: ReturnType<typeof vi.fn>;
  };
  let router: {
    navigate: ReturnType<typeof vi.fn>;
  };

  beforeEach(async () => {
    authService = {
      logout: vi.fn().mockReturnValue(of(void 0)),
    };

    router = {
      navigate: vi.fn(),
    };

    await TestBed.configureTestingModule({
      imports: [UserDashboard],
      providers: [
        { provide: AuthService, useValue: authService },
        { provide: Router, useValue: router },
      ],
    })
      .overrideComponent(UserDashboard, {
        set: { template: '' },
      })
      .compileComponents();

    const fixture = TestBed.createComponent(UserDashboard);
    component = fixture.componentInstance;
    fixture.detectChanges();
  });

  it('should navigate to the profile page', () => {
    component.openProfile();

    expect(router.navigate).toHaveBeenCalledWith(['/profile']);
  });

  it('should log out and navigate to the login page', () => {
    component.onLogout();

    expect(authService.logout).toHaveBeenCalled();
    expect(router.navigate).toHaveBeenCalledWith(['/login']);
  });

  it('should handle a failed logout without navigating', () => {
    const error = new Error('Logout failed');
    const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => undefined);

    try {
      authService.logout.mockReturnValue(throwError(() => error));

      component.onLogout();

      expect(consoleSpy).toHaveBeenCalledWith('Logout failed:', error);
      expect(router.navigate).not.toHaveBeenCalled();
    } finally {
      consoleSpy.mockRestore();
    }
  });
});
