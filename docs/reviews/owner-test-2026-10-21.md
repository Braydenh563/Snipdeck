# Owner Windows test notes, 2026-10-21 (verbatim from V1_Test.docx; image names refer to the owner's docx, not in the repo)

V1 Test: 
If I press ctrl alt space when the panel is showing, does it close it instead? (edit: no it doesn’t) Maybe just alt tab… have to manually click to close it. 
====================================================================================================
  Part 1 of 4: Paste and hotkeys (the part that talks to Windows)
  10 steps to do in this part
====================================================================================================
  (M2)
  Before you start:
    - Start this wizard from a normal (non-admin) window and snap it to the left half of the screen
      (Win+Left).
    - Turn on clipboard history: press Win+V, then 'Turn on' (needed to check that pasted items
      never show up there).
    - Open a normal (non-admin) Notepad. Have Word, Chrome (signed in to Gmail), Paint, an Explorer
      folder and Windows Terminal ready. Optional: OneNote, Remote Desktop, Ditto or CopyQ,
      PowerToys.
    - The harness has no window; it registers global hotkeys: Ctrl+Alt+1 paste text, 2 HTML, 3
      plain, 4 copy, 5 PNG, 6 files, 7 print Target info, 8 long unicode, Q quit. Focus the target
      app BEFORE pressing a chord and hold the keys until the paste happens (that exercises the Alt
      mask).
    - It never prints clipboard content or window titles (title length only). ? then l shows its
      log.
 [img: image1.png]
  P = it worked      F = it didn't      S = skip      ? = more options
    N = add a note              C = take a screenshot
    L = show what the app printed   R = restart the app
    B = go back one step        Q = stop here (continue later)
    Ctrl+C is turned off here. To copy text from this window: select it, then right-click.
  &gt; l
  --- what the app printed (last lines) ---
    [m2] chords: " ctrl + ALT + 1 " -&gt; Ctrl+Alt+1 (mods=0x4003 vk=0x31); "Win+Shift+F5" -&gt; Shift+Win+F5 (mods=0x400c vk=0x74); "Shift+A" -&gt; rejected (Shift alone is not enough for A: add Ctrl, Alt or Win); "A" -&gt; rejected (A needs at least one modifier (Ctrl, Alt, Shift or Win))
    [m2] hotkeys registered=9 in-use=0 errors=0
    [m2] hotkey Ctrl+Alt+1 registered: paste plain text item
    [m2] hotkey Ctrl+Alt+2 registered: paste HTML item (html + text)
    [m2] hotkey Ctrl+Alt+3 registered: plain paste of the HTML item (text only)
    [m2] hotkey Ctrl+Alt+4 registered: copy mode (text item stays on the clipboard)
    [m2] hotkey Ctrl+Alt+5 registered: paste 64x64 PNG with a transparent corner
    [m2] hotkey Ctrl+Alt+6 registered: paste two small text files
    [m2] hotkey Ctrl+Alt+7 registered: print Target info only
    [m2] hotkey Ctrl+Alt+8 registered: paste long multi-line unicode text
    [m2] hotkey Ctrl+Alt+Q registered: quit
    [m2] ready: focus the target app, then press a chord (the chord keys stay held, which exercises the Alt mask)
  -----------------------------------------
I opened the about snipdeck popup then the small popup wen mostly transparent accept dor a gray transparent box. Then a couple header elements showed. I clicked on the box and then it went back to normal again
 [img: image2.png]
I dragged the panel around and a small column on the right went transparent for a bit then went back to normal
 [img: image3.png]
These button options below the quick add in the small panel are so close to each other I thought they were a 3 random word sentence
 [img: image4.png]
I also think the quick add should have a second create button so one for just creating one, and the other like "add/cr## 6. Undo / redo (new, MANUAL) it does but if I want to do it one off then it is annoying to have to close it.
 [img: image5.png]
The small panel glitched temporarily again.
 [img: image6.png]
 [img: image7.png]
Troubleshooter scans should be noted on the ui and datetime recorded, If I press check again then it should say that it checked and update the last checked time
Im pressing ctrl alt space and It isnt showing now??
Visual ui clashes
 [img: image8.png]
How do I insta paste it in?? Is it shift click?? 
I want to be able to alt tab to the small window if I want as that is how I prefer to navigate sometimes
I had the big window open and then pressed to edit tags and the window did nicely appear to the right of the window but as it is so big it appeared halfway off screen. In that case it would probably be nicer to have it appear a little more central or to like the top left still kinda center  or just the centre or smth idk what is professional and modern
 [img: image9.png]
The tags panel was cut off the screen so I dragged it but then the ui that was hidden didn’t exist, it was just transparent
 [img: image10.png]
Idk but scrolling on the settings panel and maybe others feels slightly laggy and not smooth?
I want to be able to delete backups
 [img: image11.png]
I cant click off to deselect the dropdown combobox and not show the border on it
 [img: image12.png]
The scrollbar on the command palatte visually clashes and partially covers the content items
 [img: image13.png]
 [img: image14.png]
Docking to the edge 
I try to dock the small panel to the edge of the screen, it straight up covers a 3rd of my screen and the bototm ent invisible. What if I don’t want it at full page height and want it to basically stay as it is with the small window but just docked to the edge of the screen and collapsed to s small bar what when I go to hover over it  it appears temporarily and I can use it…
 [img: image15.png]
The small popup panel is still really annoying to access and dismiss quickly
There are two copy buttons
 [img: image16.png]
Might have been because it didn’t register onenote as a pastable environment
 [img: image17.png]
Im typing in onenote but it wont enter my snippet?
 [img: image18.png]
I want to be able to drag and drop snippets from the small window onto the application on my screen with a little carret marker appearing to show where it will paste as I hover over and then I can drop it where I want
When I press edit the window appears but it is soooo much taller than it needs to be and takes up excessive height and space on my screen becoming a hindrance
 [img: image19.png]
Also when I opened th epanel the small panel dissapeared and glitched visually again
I realised the edit item panel was that height because of the more options button but I think it should only expand to that height if the user sclicks the more options button and then collapse if the user hides the dropdown again. Also the window should still adjust to fit on the screen if the user opens the more options button to see the other elements
 [img: image20.png]
I tried to paste a screenshot I just took into a new snippet but it didn’t upload?? There was no failed notice or anything
Testtesttesttesttest wait now the app is recognising onenote as a text pastable environment??
