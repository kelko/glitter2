glitter
========

Template Processing
---------------------

### Goal
With `glitter` I tried to create a template processor which enables the user to 
create flexible, configurable templates. 

This is a complete rewrite of the original ruby glitter in rust with some new features and YAML as base format for configuration files.

### File Structure
Each glitter processing file is a YAML that consists of up to four top-level parts:

* `global`
* `local`
* `injection`
* `template`

The global, local and injection parts are similar to each other - as all of them define variables that can be used in other places. But they differ in accessibility and scope:

- `injection`:  The variables defined in injection are the only directly accesible from the template. Different to `global` and `local` this is not a dictionary/hash but an array of dictionary/hash. There can be multiple iterations defined - but at least one is necessary.
- `local`: See this as your workbench. You can define multiple variables either directly or by importing/loading/rendering other files. In the `injection` block you can then selectivly access those variables you need from your `local` storage
- `global`: Basically this is very similar to the `local` storage, only it is only run on the top-most glitter file. Global definitions on sub-files run using `render` are ignored. And all variables defined in `global` are accessible in all sub-files (load & render), while `local`-defined variables need to be passed along explicitly.


### Template Definition

The `template` block can either be one string, which then is processed as often as there are injections. Alternatively you can define a `header`, `body` and `footer` which can all either be direct `value` definitions or file `quote`s.

The value replacement searches for either `*{x}` constructs, which then applies the value of variable `x` in that place - or via `*>` the rest of the line is taken as variable name and replaced accordingly.


### Lazy Evaluation

All variable declarations are not processed directly when found, but only the definition is stored. Only when the variables value is actively accessed (either by using a sub-variable or by printing it into the output) is the variable actually interpreted.

Beware: A side effect of this is, that the same evaluation can happen multiple times. This might change in a later version.


### How To
A real How-To/Manual will be written, but the example files in the example-subfolder give
already an impression what `glitter` is able to do.
