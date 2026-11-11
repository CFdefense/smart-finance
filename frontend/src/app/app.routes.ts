import { Routes } from '@angular/router';
import { Login } from '../login/login';
import { Signup } from './signup/signup';
import { UserDashboard } from '../dashboard/dashboard';
import { UserProfile } from '../profile/profile';
import { GoalsPage } from '../goals/goals';
import { Budgets } from '../budgets/budgets';
import { AppLayout } from './app-layout/app-layout';
import { authGuard } from './auth-guard';
import { Landing } from './landing/landing';
import { PageNotFound } from './page-not-found/page-not-found';

export const routeConfig: Routes = [
  // Redirect visitors from the root URL to Landing.
  {
    path: '',
    redirectTo: 'landing',
    pathMatch: 'full',
  },

  // Public pages do not use the shared layout.
  {
    path: 'landing',
    component: Landing,
  },
  {
    path: 'login',
    component: Login,
  },
  {
    path: 'signup',
    component: Signup,
  },

  // All authenticated pages share the same layout.
  {
    path: '',
    component: AppLayout,

    // Protect access to the shared layout.
    canActivate: [authGuard],

    children: [
      {
        path: 'dashboard',
        component: UserDashboard,
      },
      {
        path: 'goals',
        component: GoalsPage,
      },
      {
        path: 'profile',
        component: UserProfile,
      },

      {
        path: 'budgets',
        component: Budgets,
      },

      // We'll add /budgets and /rules here
      // when those pages are implemented.
    ],
  },
  { path: '**', component: PageNotFound }
];
