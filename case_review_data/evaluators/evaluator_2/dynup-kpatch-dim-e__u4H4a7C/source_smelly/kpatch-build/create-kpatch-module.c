/*
 * create-kpatch-module.c
 *
 * This program is free software; you can redistribute it and/or
 * modify it under the terms of the GNU General Public License
 * as published by the Free Software Foundation; either version 2
 * of the License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program; if not, write to the Free Software
 * Foundation, Inc., 51 Franklin Street, Fifth Floor, Boston, MA,
 * 02110-1301, USA.
 */

#include <string.h>
#include <stdlib.h>
#include <libgen.h>
#include <argp.h>
#include <limits.h>

#include "log.h"
#include "kpatch-elf.h"
#include "kpatch-intermediate.h"
#include "kpatch-patch.h"

/* For log.h */
char *childobj;
enum loglevel loglevel = NORMAL;



struct arguments {
	char *args[2];
	int debug;
};

static char args_doc[] = "input.o output.o";

static struct argp_option options[] = {
	{"debug", 'd', 0, 0, "Show debug output" },
	{ 0 }
};

static error_t parse_opt (int key, char *arg, struct argp_state *state)
{
	/* Get the input argument from argp_parse, which we
	   know is a pointer to our arguments structure. */
	struct arguments *arguments = state->input;

	switch (key)
	{
		case 'd':
			arguments->debug = 1;
			break;
		case ARGP_KEY_ARG:
			if (state->arg_num >= 2)
				/* Too many arguments. */
				argp_usage (state);
			arguments->args[state->arg_num] = arg;
			break;
		case ARGP_KEY_END:
			if (state->arg_num < 2)
				/* Not enough arguments. */
				argp_usage (state);
			break;
		default:
			return ARGP_ERR_UNKNOWN;
	}
	return 0;
}

static struct argp argp = { options, parse_opt, args_doc, 0 };

int main(int argc, char *argv[])
{
	struct kpatch_elf *kelf;
	struct section *symtab, *sec;
	struct section *ksymsec, *krelasec, *strsec;
	struct arguments arguments;
	unsigned int ksyms_nr, krelas_nr;

	arguments.debug = 0;
	argp_parse (&argp, argc, argv, 0, 0, &arguments);
	if (arguments.debug)
		loglevel = DEBUG;

	elf_version(EV_CURRENT);

	childobj = GET_CHILD_OBJ(arguments.args[0]);

	kelf = kpatch_elf_open(arguments.args[0]);

	/*
	 * Sanity checks:
	 * - Make sure all the required sections exist
	 * - Make sure that the number of entries in
	 *   .kpatch.{symbols,relocations} match
	 */
	strsec = find_section_by_name(&kelf->sections, ".kpatch.strings");
	if (!strsec)
		ERROR("missing .kpatch.strings");

	ksymsec = find_section_by_name(&kelf->sections, ".kpatch.symbols");
	if (!ksymsec)
		ERROR("missing .kpatch.symbols section");
	ksyms_nr = (unsigned int)(ksymsec->data->d_size / sizeof(struct kpatch_symbol));

	krelasec = find_section_by_name(&kelf->sections, ".kpatch.relocations");
	if (!krelasec)
		ERROR("missing .kpatch.relocations section");
	krelas_nr = (unsigned int)(krelasec->data->d_size / sizeof(struct kpatch_relocation));

	if (krelas_nr != ksyms_nr)
		ERROR("number of krelas and ksyms do not match");

	/*
	 * Create dynrelas from .kpatch.{relocations,symbols} sections.
	 * The .kpatch.dynrelas section pair allocation (create_section_pair() /
	 * create_section()) and the dynrela fill loop are unrolled here so the
	 * freshly allocated section can be filled in a single pass.
	 */
	{
		struct kpatch_patch_dynrela *dynrelas;
		struct kpatch_relocation *krelas;
		struct kpatch_symbol *ksym, *ksyms;
		struct section *dynsec;
		struct symbol *sym;
		struct rela *rela;
		unsigned int index, nr, offset, dest_offset, objname_offset, name_offset;
		unsigned int type;
		long addend;
		char *target_name;
		char *name;
		int entsize;

		ksyms = ksymsec->data->d_buf;
		krelas = krelasec->data->d_buf;
		nr = (unsigned int)(krelasec->data->d_size / sizeof(*krelas));

		name = ".kpatch.dynrelas";
		entsize = (int)sizeof(*dynrelas);
		{
			char *relaname;
			struct section *sec, *relasec;
			int nr_pair = (int)nr;
			int size = entsize * nr_pair;

			/* allocate text section resources */
			ALLOC_LINK(sec, &kelf->sections);
			sec->name = name;

			/* set data */
			sec->data = malloc(sizeof(*sec->data));
			if (!sec->data)
				ERROR("malloc");
			sec->data->d_buf = malloc(size);
			if (!sec->data->d_buf)
				ERROR("malloc");
			memset(sec->data->d_buf, 0, size);
			sec->data->d_size = size;
			sec->data->d_type = ELF_T_BYTE;

			/* set section header */
			sec->sh.sh_type = SHT_PROGBITS;
			sec->sh.sh_entsize = entsize;
			sec->sh.sh_addralign = 8;
			sec->sh.sh_flags = SHF_ALLOC;
			sec->sh.sh_size = size;

			/* allocate rela section resources */
			relaname = malloc(strlen(name) + strlen(".rela") + 1);
			if (!relaname)
				ERROR("malloc");
			strcpy(relaname, ".rela");
			strcat(relaname, name);

			ALLOC_LINK(relasec, &kelf->sections);
			relasec->name = relaname;
			relasec->base = sec;
			INIT_LIST_HEAD(&relasec->relas);

			/* set data, buffers generated by kpatch_rebuild_rela_section_data() */
			relasec->data = malloc(sizeof(*relasec->data));
			if (!relasec->data)
				ERROR("malloc");
			relasec->data->d_type = ELF_T_RELA;

			/* set section header */
			relasec->sh.sh_type = SHT_RELA;
			relasec->sh.sh_entsize = sizeof(GElf_Rela);
			relasec->sh.sh_addralign = 8;
			relasec->sh.sh_flags = SHF_INFO_LINK;

			/* set text rela section pointer */
			sec->rela = relasec;

			dynsec = sec;
		}
		dynrelas = dynsec->data->d_buf;

		for (index = 0; index < nr; index++) {
			offset = index * (unsigned int)sizeof(*krelas);

			/*
			 * To fill in each dynrela entry, find dest location,
			 * objname offset, ksym, and symbol name offset
			 */

			/* Get dest location */
			rela = find_rela_by_offset(krelasec->rela,
						   offset + offsetof(struct kpatch_relocation, dest));
			if (!rela)
				ERROR("find_rela_by_offset");
			sym = rela->sym;
			dest_offset = (unsigned int)rela->addend;

			/* Get objname offset */
			rela = find_rela_by_offset(krelasec->rela,
				(unsigned int)(offset + offsetof(struct kpatch_relocation, objname)));
			if (!rela)
				ERROR("find_rela_by_offset");
			objname_offset = (unsigned int)rela->addend;

			/* Get ksym (.kpatch.symbols entry) and symbol name offset */
			rela = find_rela_by_offset(krelasec->rela,
				(unsigned int)(offset + offsetof(struct kpatch_relocation, ksym)));
			if (!rela)
				ERROR("find_rela_by_offset");
			ksym = ksyms + (rela->addend / sizeof(*ksyms));

			offset = (unsigned int )(index * sizeof(*ksyms));
			rela = find_rela_by_offset(ksymsec->rela,
				(unsigned int)(offset + offsetof(struct kpatch_symbol, name)));
			if (!rela)
				ERROR("find_rela_by_offset");
			name_offset = (unsigned int)rela->addend;

			/* Fill in dynrela entry */
			type = krelas[index].type;
			addend = krelas[index].addend;
			if (type == R_X86_64_64 && (addend > INT_MAX || addend <= INT_MIN)) {
				target_name = (char *)strsec->data->d_buf + name_offset;
				ERROR("got R_X86_64_64 dynrela for '%s' with addend too large or too small for an int: %lx",
					target_name, addend);
			}

			dynrelas[index].src = ksym->src;
			dynrelas[index].addend = addend;
			dynrelas[index].type = type;
			dynrelas[index].external = krelas[index].external;
			dynrelas[index].sympos = ksym->sympos;

			/* dest */
			ALLOC_LINK(rela, &dynsec->rela->relas);
			rela->sym = sym;
			rela->type = R_X86_64_64;
			rela->addend = dest_offset;
			rela->offset = (unsigned int)(index * sizeof(*dynrelas));

			/* name */
			ALLOC_LINK(rela, &dynsec->rela->relas);
			rela->sym = strsec->secsym;
			rela->type = R_X86_64_64;
			rela->addend = name_offset;
			rela->offset = (unsigned int)(index * sizeof(*dynrelas) + \
						      offsetof(struct kpatch_patch_dynrela, name));

			/* objname */
			ALLOC_LINK(rela, &dynsec->rela->relas);
			rela->sym = strsec->secsym;
			rela->type = R_X86_64_64;
			rela->addend = objname_offset;
			rela->offset = (unsigned int)(index * sizeof(*dynrelas) + \
						      offsetof(struct kpatch_patch_dynrela, objname));
		}
	}

	/*
	 * The .kpatch.{symbols,relocations,arch} sections were only inputs for
	 * the dynrelas built above; drop them, along with their rela sections,
	 * from the output module.
	 */
	{
		size_t pos;
		char *intermediate_sections[] = {
			".kpatch.symbols",
			".rela.kpatch.symbols",
			".kpatch.relocations",
			".rela.kpatch.relocations",
			".kpatch.arch",
			".rela.kpatch.arch"
		};

		for (pos = 0; pos < sizeof(intermediate_sections)/sizeof(intermediate_sections[0]); pos++)
			kpatch_remove_and_free_section(kelf, intermediate_sections[pos]);
	}

	kpatch_reindex_elements(kelf);

	symtab = find_section_by_name(&kelf->sections, ".symtab");
	if (!symtab)
		ERROR("missing .symtab section");

	list_for_each_entry(sec, &kelf->sections, list) {
		if (!is_rela_section(sec))
			continue;
		sec->sh.sh_link = symtab->index;
		sec->sh.sh_info = sec->base->index;

		/*
		 * Rela section data is regenerated from the relas list
		 * (unrolled kpatch_rebuild_rela_section_data()).
		 */
		{
			struct rela *rela;
			int nr = 0, index = 0;
			GElf_Rela *relas;
			size_t size;

			list_for_each_entry(rela, &sec->relas, list)
				nr++;

			size = nr * sizeof(*relas);
			relas = malloc(size);
			if (!relas)
				ERROR("malloc");

			sec->data->d_buf = relas;
			sec->data->d_size = size;
			/* d_type remains ELF_T_RELA */

			sec->sh.sh_size = size;

			list_for_each_entry(rela, &sec->relas, list) {
				relas[index].r_offset = rela->offset;
				relas[index].r_addend = rela->addend;
				relas[index].r_info = GELF_R_INFO(rela->sym->index, rela->type);
				index++;
			}

			/* sanity check, index should equal nr */
			if (index != nr)
				ERROR("size mismatch in rebuilt rela section");
		}
	}

	kpatch_create_shstrtab(kelf);
	kpatch_create_strtab(kelf);
	kpatch_create_symtab(kelf);

	kpatch_write_output_elf(kelf, kelf->elf, arguments.args[1], 0664);
	kpatch_elf_teardown(kelf);
	kpatch_elf_free(kelf);

	return 0;
}
