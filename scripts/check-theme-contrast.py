#!/usr/bin/env python3
# Verifica contraste WCAG de los temas: python scripts/check-theme-contrast.py resources/themes
import tomllib, glob, sys
def L(h):
    h=h.lstrip('#'); r,g,b=[int(h[i:i+2],16)/255 for i in (0,2,4)]
    f=lambda c: c/12.92 if c<=0.03928 else ((c+0.055)/1.055)**2.4
    return 0.2126*f(r)+0.7152*f(g)+0.0722*f(b)
def cr(a,b):
    x,y=sorted([L(a),L(b)],reverse=True); return (x+0.05)/(y+0.05)
bad=0
for p in sorted(glob.glob(sys.argv[1]+'/*.toml')):
    t=tomllib.load(open(p,'rb')); c=t['colors']; s=t['style']; a=t['age']; ft=t['filetypes']; tm=t['terminal']
    checks=[('fg/pane',c['fg'],c['pane_bg'],7),('fg/alt',c['fg'],c['pane_alt_row'],7),('muted/pane',c['fg_muted'],c['pane_bg'],4.5),
     ('sidebar',c['sidebar_fg'],c['sidebar_bg'],4.5),('muted/sidebar',c['fg_muted'],c['sidebar_bg'],4.5),('header',c['header_fg'],c['header_bg'],4.5),
     ('accent_fg/accent',c['accent_fg'],c['accent'],4.5),('sel',c['selection_fg'],c['selection_bg'],4.5),('fg/sel_inact',c['fg'],c['selection_inactive_bg'],4.5),
     ('link/pane',c['link'],c['pane_bg'],4.5),('focus/pane',c['focus_ring'],c['pane_bg'],3),('active_border/pane',c['pane_active_border'],c['pane_bg'],3),
     ('border_strong/pane',c['border_strong'],c['pane_bg'],1.6),('term fg/bg',tm['foreground'],tm['background'],7),('term sel',tm['selection_fg'],tm['selection_bg'],4.5),
     ('cursor/bg',tm['cursor'],tm['background'],3)]
    for k in ('success','warning','error','info'): checks.append((k+'/pane',c[k],c['pane_bg'],4.5))
    for k in ('hour','day','week','month','year'):
        if s['age_style']=='chip': checks.append(('age '+k,a['chip_fg'],a[k],4.5))
        else: checks.append(('age txt '+k,a[k],c['pane_bg'],4.5))
    for k,v in ft.items(): checks.append(('ft '+k,v,c['pane_bg'],3))
    for i in range(1,7): checks.append((f'pal{i}',tm['palette'][i],tm['background'],3))
    for i in range(9,15): checks.append((f'pal{i}',tm['palette'][i],tm['background'],3))
    fails=[(n,round(cr(x,y),2),m) for n,x,y,m in checks if cr(x,y)<m]
    bad+=len(fails); print(t['meta']['id'], 'OK' if not fails else fails)
sys.exit(1 if bad else 0)
