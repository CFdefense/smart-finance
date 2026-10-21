import { Routes } from '@angular/router';
import { Login } from '../login/login';
import { UserDashboard } from '../dashboard/dashboard';
import { UserProfile } from '../profile/profile';
import { Signup } from './signup/signup';
import { authGuard } from './auth-guard';

export const routeConfig: Routes = [
    {path: '', redirectTo: 'login', pathMatch: 'full'},
    {path: 'login', component: Login},
    {path: 'signup', component: Signup},
    {path: 'dashboard', component: UserDashboard, canActivate: [authGuard]},
    {path: 'profile', component: UserProfile, canActivate: [authGuard]}
];
