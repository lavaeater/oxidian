# UI Polish

The UI is serviceable, but not "perfect". But what is perfection?

Perfection is probably sensible defaults, smooth editing in our case, and then perhaps easy customization.

## Small issues

### Caret Stuck in Checkbox Lists

If the caret is in a checkbox list between the dash "-" and the left square bracket "[" it cannot scroll up or down. This makes flowing through a document slow.

### Conserve Vertical space

Toolbars always tak up space. 
In our app we have tabs at the top that indicate which document we are working on. 
Below that we then have the "status" bar with file name / path, a button for the command palette, add bookmark, word count
move to other pane and an export to html button.
And below that we have the properties pane - collapsible, but defaults to open.
Lastly we have the formatting toolbar with some common formatting tasks - however, none of the buttons seem to work.

Solution: I want us to add a top right button that reveals a menu with a checkbox list of what bars to actually show.
Bars should also be placeable at the top OR bottom - the status bar for instance would work better at the bottom by
default. It should also have a menu option called Super Collapse that simply hides all the bars except the top one for 
the tabs and the button itself.

Formatting toolbar: why aren't the buttons working?

### Modify Size of Sidebar

I ran into the issue the other day that the sidebar was to "short" to fit the document list in a convenient manner. 
This was because the outline took up a lot of space. One way around this is to use the width of the screen instead, i.e.
making sidebars moveable to the left or right of the writing area. If we have two panes for writing, a bar would go to
the far right or far left of both of the writing areas.

### Jumpy UX

I notice quite often when working with checkbox lists, say if I have written a shopping list, that the UI jumps around 
quite a bit when checking items - on my phone - so that would be in the actual mobile app. So I bring up the shopping list, in this instance I just want to check 
items off as a buy groceries, but every time I check an item the keyboard pops up and I am taken on a whirlwind travel 
across the document. Annoying.
