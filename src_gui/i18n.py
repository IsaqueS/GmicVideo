"""
Internationalization (i18n) Engine for G'MIC Video
Supports English (en), Portuguese (pt), Spanish (es), and Japanese (ja).
Automatically detects and defaults to the host system's language.
"""

import os
import sys
import locale
from PySide6.QtCore import QObject, Signal, Property, Slot
from PySide6.QtCore import QLocale

TRANSLATIONS = {
    "en": {
        # General & App Header
        "G'MIC Video - Import Screen": "G'MIC Video - Import Screen",
        "G'MIC Video - Effects Workspace": "G'MIC Video - Effects Workspace",
        "Import Video": "Import Video",
        "Import Video File...": "Import Video File...",
        "Import a Video to Apply G'MIC Effects": "Import a Video to Apply G'MIC Effects",
        "Select a video to start adding animated G'MIC effects with custom tweening, easing and curves.": "Select a video to start adding animated G'MIC effects with custom tweening, easing and curves.",
        "Load Sample: Original.mp4": "Load Sample: Original.mp4",
        "Render": "Render",
        "Render (%1 in queue | %2%)": "Render (%1 in queue | %2%)",
        "Language": "Language",

        # Left Column - Categories & Effects
        "Search effects...": "Search effects...",
        "Visual": "Visual",
        "Category:": "Category:",
        "Effects:": "Effects:",
        "All Categories": "All Categories",

        # Center Column - Effect Parameters & Presets
        "Select an Effect": "Select an Effect",
        "Save .geffect...": "Save .geffect...",
        "Load .geffect...": "Load .geffect...",
        "Reset Defaults": "Reset Defaults",
        "Customize effect parameters and tween values over the video duration.": "Customize effect parameters and tween values over the video duration.",
        "Saved preset to: %1": "Saved preset to: %1",
        "Failed to load preset: %1": "Failed to load preset: %1",
        "Loaded preset: %1": "Loaded preset: %1",
        "Preset parse error: %1": "Preset parse error: %1",
        "Preset requires filter '%1' which is not found!": "Preset requires filter '%1' which is not found!",
        "Animate": "Animate",
        "Animate: Random": "Animate: Random",
        "Animate: Curve": "Animate: Curve",
        "Disabled (Static Value)": "Disabled (Static Value)",
        "Random (New Value Every Frame)": "Random (New Value Every Frame)",
        "Curve (Keyframe & Ease Curve)": "Curve (Keyframe & Ease Curve)",
        "Random Mode Active: Generating pseudo-random values between %1 and %2 on every video frame.": "Random Mode Active: Generating pseudo-random values between %1 and %2 on every video frame.",
        "Curve Preset:": "Curve Preset:",
        "Curve Amplitude (Range Limits):": "Curve Amplitude (Range Limits):",
        "Min Limit: %1": "Min Limit: %1",
        "Max Limit: %1": "Max Limit: %1",

        # Presets (Curve Easing)
        "Linear (Default)": "Linear (Default)",
        "Linear": "Linear",
        "Ease In (Quad)": "Ease In (Quad)",
        "Ease Out (Quad)": "Ease Out (Quad)",
        "Ease In-Out (Quad)": "Ease In-Out (Quad)",
        "Ease In (Cubic)": "Ease In (Cubic)",
        "Ease Out (Cubic)": "Ease Out (Cubic)",
        "Ease In-Out (Cubic)": "Ease In-Out (Cubic)",
        "Ease In (Quart)": "Ease In (Quart)",
        "Ease Out (Quart)": "Ease Out (Quart)",
        "Ease In-Out (Quart)": "Ease In-Out (Quart)",
        "Ease In (Sine)": "Ease In (Sine)",
        "Ease Out (Sine)": "Ease Out (Sine)",
        "Ease In-Out (Sine)": "Ease In-Out (Sine)",
        "Ease In (Expo)": "Ease In (Expo)",
        "Ease Out (Expo)": "Ease Out (Expo)",
        "Ease In-Out (Expo)": "Ease In-Out (Expo)",
        "Ease In (Circ)": "Ease In (Circ)",
        "Ease Out (Circ)": "Ease Out (Circ)",
        "Ease In-Out (Circ)": "Ease In-Out (Circ)",
        "Ease In (Back/Overshoot)": "Ease In (Back/Overshoot)",
        "Ease Out (Back/Overshoot)": "Ease Out (Back/Overshoot)",
        "Ease In-Out (Back)": "Ease In-Out (Back)",
        "Ease In (Elastic)": "Ease In (Elastic)",
        "Ease Out (Elastic)": "Ease Out (Elastic)",
        "Ease In-Out (Elastic)": "Ease In-Out (Elastic)",
        "Ease In (Bounce)": "Ease In (Bounce)",
        "Ease Out (Bounce)": "Ease Out (Bounce)",
        "Ease In-Out (Bounce)": "Ease In-Out (Bounce)",
        "Custom Editable Bezier": "Custom Editable Bezier",
        "Custom Bezier Curve": "Custom Bezier Curve",

        # Right Column - Preview & Scrubber
        "Preview": "Preview",
        "Split View": "Split View",
        "Full View": "Full View",
        "Refresh Preview": "Refresh Preview",
        "Original": "Original",
        "G'MIC Effect": "G'MIC Effect",
        "Frame: %1 / %2": "Frame: %1 / %2",
        "Cut Range (Trim Video):": "Cut Range (Trim Video):",
        "Reset Cut to Full Video": "Reset Cut to Full Video",
        "Start: %1s": "Start: %1s",
        "End: %1s": "End: %1s",
        "Cut Range: %1s -> %2s (Duration: %3s)": "Cut Range: %1s -> %2s (Duration: %3s)",

        # Visual Search Window
        "Visual Effect Search - G'MIC Video": "Visual Effect Search - G'MIC Video",
        "Search effects visually (e.g. bokeh, warp, glitch, blur, halftone)...": "Search effects visually (e.g. bokeh, warp, glitch, blur, halftone)...",
        "With Previews Only": "With Previews Only",
        "Close": "Close",
        "Showing %1 effects": "Showing %1 effects",
        "Showing %1 effects in %2": "Showing %1 effects in %2",
        "Click any effect card to apply it to your video": "Click any effect card to apply it to your video",
        "No matching effects found": "No matching effects found",
        "Try adjusting your search query or selecting 'All Categories'.": "Try adjusting your search query or selecting 'All Categories'.",

        # Render Dialog & Queue
        "Render Manager & Task Queue - G'MIC Video": "Render Manager & Task Queue - G'MIC Video",
        "New Render": "New Render",
        "Task Queue (%1)": "Task Queue (%1)",
        "Render Source & Cut Range:": "Render Source & Cut Range:",
        "Output Destination File:": "Output Destination File:",
        "Browse...": "Browse...",
        "Encoding Settings (Quality & Speed):": "Encoding Settings (Quality & Speed):",
        "Encoding Quality:": "Encoding Quality:",
        "Encoding Speed:": "Encoding Speed:",
        "Lossless RGB encoding active: Quality and speed presets are bypassed to guarantee exact 1:1 mathematical pixel fidelity.": "Lossless RGB encoding active: Quality and speed presets are bypassed to guarantee exact 1:1 mathematical pixel fidelity.",
        "When Rendering Completes:": "When Rendering Completes:",
        "Do nothing": "Do nothing",
        "Shutdown computer": "Shutdown computer",
        "Add to Render Queue & Start": "Add to Render Queue & Start",
        "Render Queue (%1 items)": "Render Queue (%1 items)",
        "Clear Finished": "Clear Finished",
        "No render tasks in queue": "No render tasks in queue",
        "Use the 'New Render' tab to add a video rendering task to the background queue.": "Use the 'New Render' tab to add a video rendering task to the background queue.",
        "Cancel": "Cancel",
        "Open Folder": "Open Folder",
        "Play Video": "Play Video",
        "Close Window": "Close Window",
        "Ready - Rendering runs independently in the background": "Ready - Rendering runs independently in the background",
        "%1 active tasks rendering in background...": "%1 active tasks rendering in background...",
        "Draft Quality (Faster, larger compression artifacts)": "Draft Quality (Faster, larger compression artifacts)",
        "Standard Quality (Balanced, recommended for sharing)": "Standard Quality (Balanced, recommended for sharing)",
        "High Quality (Crisp details, high bitrate - Recommended)": "High Quality (Crisp details, high bitrate - Recommended)",
        "Maximum Quality (Visually lossless, large file size)": "Maximum Quality (Visually lossless, large file size)",
        "Maximum Compression (Slowest encode, smallest file)": "Maximum Compression (Slowest encode, smallest file)",
        "Slow (Thorough compression)": "Slow (Thorough compression)",
        "Balanced (Medium speed and size)": "Balanced (Medium speed and size)",
        "Fast (Faster encoding - Recommended)": "Fast (Faster encoding - Recommended)",
        "Ultrafast (Instant encode, ideal for quick testing)": "Ultrafast (Instant encode, ideal for quick testing)",
        "Finished": "Finished",

        # Bezier Curve Editor Dialog
        "Curve Transition Editor": "Curve Transition Editor",
        "Cubic Bezier Transition for %1": "Cubic Bezier Transition for %1",
        "Drag the control points P1 and P2 or choose a preset to sculpt value transitions over video duration.": "Drag the control points P1 and P2 or choose a preset to sculpt value transitions over video duration.",
        "Presets:": "Presets:",
        "Ease": "Ease",
        "Ease In": "Ease In",
        "Ease Out": "Ease Out",
        "Ease In-Out": "Ease In-Out",
        "Overshoot": "Overshoot",
        "Apply Curve": "Apply Curve"
    },

    "pt": {
        # General & App Header
        "G'MIC Video - Import Screen": "G'MIC Video - Tela Inicial",
        "G'MIC Video - Effects Workspace": "G'MIC Video - Área de Efeitos",
        "Import Video": "Importar Vídeo",
        "Import Video File...": "Importar Arquivo de Vídeo...",
        "Import a Video to Apply G'MIC Effects": "Importe um Vídeo para Aplicar Efeitos G'MIC",
        "Select a video to start adding animated G'MIC effects with custom tweening, easing and curves.": "Selecione um vídeo para começar a adicionar efeitos G'MIC animados com transições, curvas e interpolações.",
        "Load Sample: Original.mp4": "Carregar Amostra: Original.mp4",
        "Render": "Renderizar",
        "Render (%1 in queue | %2%)": "Renderizar (%1 na fila | %2%)",
        "Language": "Idioma",

        # Left Column - Categories & Effects
        "Search effects...": "Pesquisar efeitos...",
        "Visual": "Visual",
        "Category:": "Categoria:",
        "Effects:": "Efeitos:",
        "All Categories": "Todas as Categorias",

        # Center Column - Effect Parameters & Presets
        "Select an Effect": "Selecione um Efeito",
        "Save .geffect...": "Salvar .geffect...",
        "Load .geffect...": "Carregar .geffect...",
        "Reset Defaults": "Restaurar Padrões",
        "Customize effect parameters and tween values over the video duration.": "Personalize os parâmetros do efeito e transições ao longo do vídeo.",
        "Saved preset to: %1": "Predefinição salva em: %1",
        "Failed to load preset: %1": "Falha ao carregar predefinição: %1",
        "Loaded preset: %1": "Predefinição carregada: %1",
        "Preset parse error: %1": "Erro ao ler predefinição: %1",
        "Preset requires filter '%1' which is not found!": "A predefinição requer o filtro '%1', que não foi encontrado!",
        "Animate": "Animar",
        "Animate: Random": "Animar: Aleatório",
        "Animate: Curve": "Animar: Curva",
        "Disabled (Static Value)": "Desativado (Valor Estático)",
        "Random (New Value Every Frame)": "Aleatório (Novo Valor a Cada Frame)",
        "Curve (Keyframe & Ease Curve)": "Curva (Keyframe e Curva de Animação)",
        "Random Mode Active: Generating pseudo-random values between %1 and %2 on every video frame.": "Modo Aleatório Ativo: Gerando valores pseudoaleatórios entre %1 e %2 a cada frame do vídeo.",
        "Curve Preset:": "Predefinição de Curva:",
        "Curve Amplitude (Range Limits):": "Amplitude da Curva (Limites de Valores):",
        "Min Limit: %1": "Limite Mín: %1",
        "Max Limit: %1": "Limite Máx: %1",

        # Presets (Curve Easing)
        "Linear (Default)": "Linear (Padrão)",
        "Linear": "Linear",
        "Ease In (Quad)": "Aceleração (Quad)",
        "Ease Out (Quad)": "Desaceleração (Quad)",
        "Ease In-Out (Quad)": "Aceleração-Desaceleração (Quad)",
        "Ease In (Cubic)": "Aceleração (Cúbica)",
        "Ease Out (Cubic)": "Desaceleração (Cúbica)",
        "Ease In-Out (Cubic)": "Aceleração-Desaceleração (Cúbica)",
        "Ease In (Quart)": "Aceleração (Quártica)",
        "Ease Out (Quart)": "Desaceleração (Quártica)",
        "Ease In-Out (Quart)": "Aceleração-Desaceleração (Quártica)",
        "Ease In (Sine)": "Aceleração (Seno)",
        "Ease Out (Sine)": "Desaceleração (Seno)",
        "Ease In-Out (Sine)": "Aceleração-Desaceleração (Seno)",
        "Ease In (Expo)": "Aceleração (Exponencial)",
        "Ease Out (Expo)": "Desaceleração (Exponencial)",
        "Ease In-Out (Expo)": "Aceleração-Desaceleração (Exponencial)",
        "Ease In (Circ)": "Aceleração (Circular)",
        "Ease Out (Circ)": "Desaceleração (Circular)",
        "Ease In-Out (Circ)": "Aceleração-Desaceleração (Circular)",
        "Ease In (Back/Overshoot)": "Aceleração (Retorno/Impulso)",
        "Ease Out (Back/Overshoot)": "Desaceleração (Retorno/Impulso)",
        "Ease In-Out (Back)": "Aceleração-Desaceleração (Retorno)",
        "Ease In (Elastic)": "Aceleração (Elástica)",
        "Ease Out (Elastic)": "Desaceleração (Elástica)",
        "Ease In-Out (Elastic)": "Aceleração-Desaceleração (Elástica)",
        "Ease In (Bounce)": "Aceleração (Salto/Quique)",
        "Ease Out (Bounce)": "Desaceleração (Salto/Quique)",
        "Ease In-Out (Bounce)": "Aceleração-Desaceleração (Salto)",
        "Custom Editable Bezier": "Bézier Editável Personalizado",
        "Custom Bezier Curve": "Curva Bézier Personalizada",

        # Right Column - Preview & Scrubber
        "Preview": "Pré-visualização",
        "Split View": "Vista Dividida",
        "Full View": "Vista Completa",
        "Refresh Preview": "Atualizar Pré-visualização",
        "Original": "Original",
        "G'MIC Effect": "Efeito G'MIC",
        "Frame: %1 / %2": "Frame: %1 / %2",
        "Cut Range (Trim Video):": "Intervalo de Corte (Aparar Vídeo):",
        "Reset Cut to Full Video": "Restaurar Vídeo Completo",
        "Start: %1s": "Início: %1s",
        "End: %1s": "Fim: %1s",
        "Cut Range: %1s -> %2s (Duration: %3s)": "Intervalo de Corte: %1s -> %2s (Duração: %3s)",

        # Visual Search Window
        "Visual Effect Search - G'MIC Video": "Busca Visual de Efeitos - G'MIC Video",
        "Search effects visually (e.g. bokeh, warp, glitch, blur, halftone)...": "Pesquise efeitos visualmente (ex.: bokeh, distorção, glitch, desfoque, meio-tom)...",
        "With Previews Only": "Apenas com Miniaturas",
        "Close": "Fechar",
        "Showing %1 effects": "Mostrando %1 efeitos",
        "Showing %1 effects in %2": "Mostrando %1 efeitos em %2",
        "Click any effect card to apply it to your video": "Clique em qualquer cartão de efeito para aplicá-lo ao vídeo",
        "No matching effects found": "Nenhum efeito correspondente encontrado",
        "Try adjusting your search query or selecting 'All Categories'.": "Tente ajustar o termo de pesquisa ou selecione 'Todas as Categorias'.",

        # Render Dialog & Queue
        "Render Manager & Task Queue - G'MIC Video": "Gerenciador de Renderização e Fila - G'MIC Video",
        "New Render": "Nova Renderização",
        "Task Queue (%1)": "Fila de Tarefas (%1)",
        "Render Source & Cut Range:": "Origem e Intervalo de Corte:",
        "Output Destination File:": "Arquivo de Destino:",
        "Browse...": "Procurar...",
        "Encoding Settings (Quality & Speed):": "Configurações de Codificação (Qualidade e Velocidade):",
        "Encoding Quality:": "Qualidade de Codificação:",
        "Encoding Speed:": "Velocidade de Codificação:",
        "Lossless RGB encoding active: Quality and speed presets are bypassed to guarantee exact 1:1 mathematical pixel fidelity.": "Codificação Lossless RGB ativa: Predefinições de qualidade e velocidade são ignoradas para garantir fidelidade matemática exata 1:1.",
        "When Rendering Completes:": "Ao Concluir a Renderização:",
        "Do nothing": "Não fazer nada",
        "Shutdown computer": "Desligar o computador",
        "Add to Render Queue & Start": "Adicionar à Fila e Iniciar",
        "Render Queue (%1 items)": "Fila de Renderização (%1 itens)",
        "Clear Finished": "Limpar Concluídas",
        "No render tasks in queue": "Nenhuma tarefa de renderização na fila",
        "Use the 'New Render' tab to add a video rendering task to the background queue.": "Utilize a aba 'Nova Renderização' para adicionar tarefas à fila em segundo plano.",
        "Cancel": "Cancelar",
        "Open Folder": "Abrir Pasta",
        "Play Video": "Reproduzir Vídeo",
        "Close Window": "Fechar Janela",
        "Ready - Rendering runs independently in the background": "Pronto - A renderização é executada em segundo plano",
        "%1 active tasks rendering in background...": "%1 tarefas ativas renderizando em segundo plano...",
        "Draft Quality (Faster, larger compression artifacts)": "Qualidade Rascunho (Mais rápido, artefatos visíveis)",
        "Standard Quality (Balanced, recommended for sharing)": "Qualidade Padrão (Equilibrada, ideal para compartilhar)",
        "High Quality (Crisp details, high bitrate - Recommended)": "Alta Qualidade (Detalhes nítidos, alta taxa de bits - Recomendado)",
        "Maximum Quality (Visually lossless, large file size)": "Qualidade Máxima (Sem perdas perceptíveis, arquivo maior)",
        "Maximum Compression (Slowest encode, smallest file)": "Compressão Máxima (Codificação mais lenta, menor arquivo)",
        "Slow (Thorough compression)": "Lento (Compressão mais precisa)",
        "Balanced (Medium speed and size)": "Equilibrado (Velocidade e tamanho médios)",
        "Fast (Faster encoding - Recommended)": "Rápido (Codificação rápida - Recomendado)",
        "Ultrafast (Instant encode, ideal for quick testing)": "Ultrarrápido (Codificação instantânea para testes)",
        "Finished": "Concluído",

        # Bezier Curve Editor Dialog
        "Curve Transition Editor": "Editor de Transição de Curva",
        "Cubic Bezier Transition for %1": "Transição Bézier Cúbica para %1",
        "Drag the control points P1 and P2 or choose a preset to sculpt value transitions over video duration.": "Arraste os pontos de controle P1 e P2 ou escolha uma predefinição para moldar os valores ao longo do vídeo.",
        "Presets:": "Predefinições:",
        "Ease": "Suave",
        "Ease In": "Aceleração",
        "Ease Out": "Desaceleração",
        "Ease In-Out": "Aceleração-Desaceleração",
        "Overshoot": "Impulso/Extrapolação",
        "Apply Curve": "Aplicar Curva"
    },

    "es": {
        # General & App Header
        "G'MIC Video - Import Screen": "G'MIC Video - Pantalla de Inicio",
        "G'MIC Video - Effects Workspace": "G'MIC Video - Espacio de Efectos",
        "Import Video": "Importar Video",
        "Import Video File...": "Importar Archivo de Video...",
        "Import a Video to Apply G'MIC Effects": "Importa un Video para Aplicar Efectos G'MIC",
        "Select a video to start adding animated G'MIC effects with custom tweening, easing and curves.": "Selecciona un video para comenzar a agregar efectos G'MIC animados con curvas y transiciones personalizadas.",
        "Load Sample: Original.mp4": "Cargar Muestra: Original.mp4",
        "Render": "Renderizar",
        "Render (%1 in queue | %2%)": "Renderizar (%1 en cola | %2%)",
        "Language": "Idioma",

        # Left Column - Categories & Effects
        "Search effects...": "Buscar efectos...",
        "Visual": "Visual",
        "Category:": "Categoría:",
        "Effects:": "Efectos:",
        "All Categories": "Todas las Categorías",

        # Center Column - Effect Parameters & Presets
        "Select an Effect": "Selecciona un Efecto",
        "Save .geffect...": "Guardar .geffect...",
        "Load .geffect...": "Cargar .geffect...",
        "Reset Defaults": "Restablecer Valores",
        "Customize effect parameters and tween values over the video duration.": "Personaliza los parámetros del efecto y las transiciones a lo largo del video.",
        "Saved preset to: %1": "Preajuste guardado en: %1",
        "Failed to load preset: %1": "Error al cargar preajuste: %1",
        "Loaded preset: %1": "Preajuste cargado: %1",
        "Preset parse error: %1": "Error al procesar preajuste: %1",
        "Preset requires filter '%1' which is not found!": "¡El preajuste requiere el filtro '%1' que no fue encontrado!",
        "Animate": "Animar",
        "Animate: Random": "Animar: Aleatorio",
        "Animate: Curve": "Animar: Curva",
        "Disabled (Static Value)": "Desactivado (Valor Estático)",
        "Random (New Value Every Frame)": "Aleatorio (Nuevo Valor Cada Fotograma)",
        "Curve (Keyframe & Ease Curve)": "Curva (Fotograma Clave y Curva)",
        "Random Mode Active: Generating pseudo-random values between %1 and %2 on every video frame.": "Modo Aleatorio Activo: Generando valores pseudoaleatorios entre %1 y %2 en cada fotograma.",
        "Curve Preset:": "Preajuste de Curva:",
        "Curve Amplitude (Range Limits):": "Amplitud de la Curva (Límites de Rango):",
        "Min Limit: %1": "Límite Mín: %1",
        "Max Limit: %1": "Límite Máx: %1",

        # Presets (Curve Easing)
        "Linear (Default)": "Lineal (Predeterminado)",
        "Linear": "Lineal",
        "Ease In (Quad)": "Aceleración (Cuad)",
        "Ease Out (Quad)": "Desaceleración (Cuad)",
        "Ease In-Out (Quad)": "Aceleración-Desaceleración (Cuad)",
        "Ease In (Cubic)": "Aceleración (Cúbica)",
        "Ease Out (Cubic)": "Desaceleración (Cúbica)",
        "Ease In-Out (Cubic)": "Aceleración-Desaceleración (Cúbica)",
        "Ease In (Quart)": "Aceleración (Cuártica)",
        "Ease Out (Quart)": "Desaceleración (Cuártica)",
        "Ease In-Out (Quart)": "Aceleración-Desaceleración (Cuártica)",
        "Ease In (Sine)": "Aceleración (Seno)",
        "Ease Out (Sine)": "Desaceleración (Seno)",
        "Ease In-Out (Sine)": "Aceleración-Desaceleración (Seno)",
        "Ease In (Expo)": "Aceleración (Exponencial)",
        "Ease Out (Expo)": "Desaceleración (Exponencial)",
        "Ease In-Out (Expo)": "Aceleración-Desaceleración (Exponencial)",
        "Ease In (Circ)": "Aceleración (Circular)",
        "Ease Out (Circ)": "Desaceleración (Circular)",
        "Ease In-Out (Circ)": "Aceleración-Desaceleración (Circular)",
        "Ease In (Back/Overshoot)": "Aceleración (Retroceso/Impulso)",
        "Ease Out (Back/Overshoot)": "Desaceleración (Retroceso/Impulso)",
        "Ease In-Out (Back)": "Aceleración-Desaceleración (Retroceso)",
        "Ease In (Elastic)": "Aceleración (Elástica)",
        "Ease Out (Elastic)": "Desaceleración (Elástica)",
        "Ease In-Out (Elastic)": "Aceleración-Desaceleración (Elástica)",
        "Ease In (Bounce)": "Aceleración (Rebote)",
        "Ease Out (Bounce)": "Desaceleración (Rebote)",
        "Ease In-Out (Bounce)": "Aceleración-Desaceleración (Rebote)",
        "Custom Editable Bezier": "Bézier Editable Personalizado",
        "Custom Bezier Curve": "Curva Bézier Personalizada",

        # Right Column - Preview & Scrubber
        "Preview": "Vista Previa",
        "Split View": "Vista Dividida",
        "Full View": "Vista Completa",
        "Refresh Preview": "Actualizar Vista Previa",
        "Original": "Original",
        "G'MIC Effect": "Efecto G'MIC",
        "Frame: %1 / %2": "Fotograma: %1 / %2",
        "Cut Range (Trim Video):": "Rango de Corte (Recortar Video):",
        "Reset Cut to Full Video": "Restablecer Video Completo",
        "Start: %1s": "Inicio: %1s",
        "End: %1s": "Fin: %1s",
        "Cut Range: %1s -> %2s (Duration: %3s)": "Rango de Corte: %1s -> %2s (Duración: %3s)",

        # Visual Search Window
        "Visual Effect Search - G'MIC Video": "Búsqueda Visual de Efectos - G'MIC Video",
        "Search effects visually (e.g. bokeh, warp, glitch, blur, halftone)...": "Busca efectos visualmente (ej.: bokeh, distorsión, glitch, desenfoque, semitono)...",
        "With Previews Only": "Solo con Miniaturas",
        "Close": "Cerrar",
        "Showing %1 effects": "Mostrando %1 efectos",
        "Showing %1 effects in %2": "Mostrando %1 efectos en %2",
        "Click any effect card to apply it to your video": "Haz clic en cualquier tarjeta para aplicar el efecto",
        "No matching effects found": "No se encontraron efectos coincidentes",
        "Try adjusting your search query or selecting 'All Categories'.": "Intenta ajustar los términos de búsqueda o selecciona 'Todas las Categorías'.",

        # Render Dialog & Queue
        "Render Manager & Task Queue - G'MIC Video": "Administrador de Render y Cola de Tareas - G'MIC Video",
        "New Render": "Nuevo Render",
        "Task Queue (%1)": "Cola de Tareas (%1)",
        "Render Source & Cut Range:": "Origen y Rango de Corte:",
        "Output Destination File:": "Archivo de Destino:",
        "Browse...": "Examinar...",
        "Encoding Settings (Quality & Speed):": "Ajustes de Codificación (Calidad y Velocidad):",
        "Encoding Quality:": "Calidad de Codificación:",
        "Encoding Speed:": "Velocidad de Codificación:",
        "Lossless RGB encoding active: Quality and speed presets are bypassed to guarantee exact 1:1 mathematical pixel fidelity.": "Codificación Lossless RGB activa: Los ajustes de calidad y velocidad se omiten para garantizar fidelidad matemática 1:1.",
        "When Rendering Completes:": "Al Completar la Renderización:",
        "Do nothing": "No hacer nada",
        "Shutdown computer": "Apagar el equipo",
        "Add to Render Queue & Start": "Añadir a la Cola e Iniciar",
        "Render Queue (%1 items)": "Cola de Render (%1 tareas)",
        "Clear Finished": "Limpiar Finalizados",
        "No render tasks in queue": "No hay tareas de render en cola",
        "Use the 'New Render' tab to add a video rendering task to the background queue.": "Usa la pestaña 'Nuevo Render' para añadir tareas en segundo plano.",
        "Cancel": "Cancelar",
        "Open Folder": "Abrir Carpeta",
        "Play Video": "Reproducir Video",
        "Close Window": "Cerrar Ventana",
        "Ready - Rendering runs independently in the background": "Listo - La renderización se ejecuta en segundo plano",
        "%1 active tasks rendering in background...": "%1 tareas activas renderizando en segundo plano...",
        "Draft Quality (Faster, larger compression artifacts)": "Calidad Borrador (Más rápido, artefactos visibles)",
        "Standard Quality (Balanced, recommended for sharing)": "Calidad Estándar (Equilibrada, ideal para compartir)",
        "High Quality (Crisp details, high bitrate - Recommended)": "Alta Calidad (Detalles nítidos, alta tasa de bits - Recomendado)",
        "Maximum Quality (Visually lossless, large file size)": "Calidad Máxima (Visualmente sin pérdida, archivo pesado)",
        "Maximum Compression (Slowest encode, smallest file)": "Máxima Compresión (Codificación lenta, archivo pequeño)",
        "Slow (Thorough compression)": "Lento (Compresión profunda)",
        "Balanced (Medium speed and size)": "Equilibrado (Velocidad y tamaño medios)",
        "Fast (Faster encoding - Recommended)": "Rápido (Codificación rápida - Recomendado)",
        "Ultrafast (Instant encode, ideal for quick testing)": "Ultrarrápido (Codificación instantánea para pruebas)",
        "Finished": "Finalizado",

        # Bezier Curve Editor Dialog
        "Curve Transition Editor": "Editor de Transición de Curva",
        "Cubic Bezier Transition for %1": "Transición Bézier Cúbica para %1",
        "Drag the control points P1 and P2 or choose a preset to sculpt value transitions over video duration.": "Arrastra los puntos de control P1 y P2 o elige un ajuste preestablecido.",
        "Presets:": "Preajustes:",
        "Ease": "Suave",
        "Ease In": "Aceleración",
        "Ease Out": "Desaceleración",
        "Ease In-Out": "Aceleración-Desaceleración",
        "Overshoot": "Impulso/Rebase",
        "Apply Curve": "Aplicar Curva"
    },

    "ja": {
        # General & App Header
        "G'MIC Video - Import Screen": "G'MIC Video - インポート画面",
        "G'MIC Video - Effects Workspace": "G'MIC Video - エフェクトワークスペース",
        "Import Video": "動画をインポート",
        "Import Video File...": "動画ファイルを選択...",
        "Import a Video to Apply G'MIC Effects": "動画をインポートしてG'MICエフェクトを適用",
        "Select a video to start adding animated G'MIC effects with custom tweening, easing and curves.": "動画を選択して、カスタムイージングやカーブによるアニメーション付きG'MICエフェクトを追加しましょう。",
        "Load Sample: Original.mp4": "サンプルを読み込む: Original.mp4",
        "Render": "レンダリング",
        "Render (%1 in queue | %2%)": "レンダリング (待機: %1 | %2%)",
        "Language": "言語",

        # Left Column - Categories & Effects
        "Search effects...": "エフェクトを検索...",
        "Visual": "ビジュアル",
        "Category:": "カテゴリー:",
        "Effects:": "エフェクト:",
        "All Categories": "すべてのカテゴリー",

        # Center Column - Effect Parameters & Presets
        "Select an Effect": "エフェクトを選択",
        "Save .geffect...": ".geffectを保存...",
        "Load .geffect...": ".geffectを読み込み...",
        "Reset Defaults": "初期値にリセット",
        "Customize effect parameters and tween values over the video duration.": "動画全体のパラメータやアニメーションカーブを細かく調整できます。",
        "Saved preset to: %1": "プリセットを保存しました: %1",
        "Failed to load preset: %1": "プリセットの読み込みに失敗しました: %1",
        "Loaded preset: %1": "プリセットを読み込みました: %1",
        "Preset parse error: %1": "プリセット解析エラー: %1",
        "Preset requires filter '%1' which is not found!": "プリセットが必要とするフィルター '%1' が見つかりません！",
        "Animate": "アニメーション",
        "Animate: Random": "アニメーション: ランダム",
        "Animate: Curve": "アニメーション: カーブ",
        "Disabled (Static Value)": "無効 (固定値)",
        "Random (New Value Every Frame)": "ランダム (フレーム毎に新しい値)",
        "Curve (Keyframe & Ease Curve)": "カーブ (キーフレーム曲線)",
        "Random Mode Active: Generating pseudo-random values between %1 and %2 on every video frame.": "ランダムモード有効: 全フレームで %1 から %2 の疑似ランダム値を生成します。",
        "Curve Preset:": "カーブプリセット:",
        "Curve Amplitude (Range Limits):": "カーブ振幅 (値の範囲制限):",
        "Min Limit: %1": "最小制限: %1",
        "Max Limit: %1": "最大制限: %1",

        # Presets (Curve Easing)
        "Linear (Default)": "リニア (標準)",
        "Linear": "リニア",
        "Ease In (Quad)": "イーズイン (2次)",
        "Ease Out (Quad)": "イーズアウト (2次)",
        "Ease In-Out (Quad)": "イーズイン・アウト (2次)",
        "Ease In (Cubic)": "イーズイン (3次)",
        "Ease Out (Cubic)": "イーズアウト (3次)",
        "Ease In-Out (Cubic)": "イーズイン・アウト (3次)",
        "Ease In (Quart)": "イーズイン (4次)",
        "Ease Out (Quart)": "イーズアウト (4次)",
        "Ease In-Out (Quart)": "イーズイン・アウト (4次)",
        "Ease In (Sine)": "イーズイン (サイン)",
        "Ease Out (Sine)": "イーズアウト (サイン)",
        "Ease In-Out (Sine)": "イーズイン・アウト (サイン)",
        "Ease In (Expo)": "イーズイン (指数)",
        "Ease Out (Expo)": "イーズアウト (指数)",
        "Ease In-Out (Expo)": "イーズイン・アウト (指数)",
        "Ease In (Circ)": "イーズイン (円)",
        "Ease Out (Circ)": "イーズアウト (円)",
        "Ease In-Out (Circ)": "イーズイン・アウト (円)",
        "Ease In (Back/Overshoot)": "イーズイン (行き過ぎ/反動)",
        "Ease Out (Back/Overshoot)": "イーズアウト (行き過ぎ/反動)",
        "Ease In-Out (Back)": "イーズイン・アウト (反動)",
        "Ease In (Elastic)": "イーズイン (弾性)",
        "Ease Out (Elastic)": "イーズアウト (弾性)",
        "Ease In-Out (Elastic)": "イーズイン・アウト (弾性)",
        "Ease In (Bounce)": "イーズイン (バウンス)",
        "Ease Out (Bounce)": "イーズアウト (バウンス)",
        "Ease In-Out (Bounce)": "イーズイン・アウト (バウンス)",
        "Custom Editable Bezier": "カスタム編集可能ベジェ",
        "Custom Bezier Curve": "カスタムベジェ曲線",

        # Right Column - Preview & Scrubber
        "Preview": "プレビュー",
        "Split View": "分割表示",
        "Full View": "全体表示",
        "Refresh Preview": "プレビュー更新",
        "Original": "オリジナル",
        "G'MIC Effect": "G'MICエフェクト",
        "Frame: %1 / %2": "フレーム: %1 / %2",
        "Cut Range (Trim Video):": "トリミング範囲 (動画カット):",
        "Reset Cut to Full Video": "全体にリセット",
        "Start: %1s": "開始: %1s",
        "End: %1s": "終了: %1s",
        "Cut Range: %1s -> %2s (Duration: %3s)": "トリミング範囲: %1s -> %2s (長さ: %3s)",

        # Visual Search Window
        "Visual Effect Search - G'MIC Video": "エフェクト視覚検索 - G'MIC Video",
        "Search effects visually (e.g. bokeh, warp, glitch, blur, halftone)...": "視覚的にエフェクトを検索 (例: bokeh, warp, glitch, blur, halftone)...",
        "With Previews Only": "プレビュー付きのみ表示",
        "Close": "閉じる",
        "Showing %1 effects": "%1 件のエフェクトを表示中",
        "Showing %1 effects in %2": "%2 内の %1 件のエフェクトを表示中",
        "Click any effect card to apply it to your video": "カードをクリックすると動画にエフェクトが適用されます",
        "No matching effects found": "該当するエフェクトが見つかりませんでした",
        "Try adjusting your search query or selecting 'All Categories'.": "検索キーワードを変更するか、「すべてのカテゴリー」を選択してください。",

        # Render Dialog & Queue
        "Render Manager & Task Queue - G'MIC Video": "レンダリング管理 & タスクキュー - G'MIC Video",
        "New Render": "新規レンダリング",
        "Task Queue (%1)": "タスクキュー (%1)",
        "Render Source & Cut Range:": "レンダリング対象 & トリミング範囲:",
        "Output Destination File:": "出力先ファイル:",
        "Browse...": "参照...",
        "Encoding Settings (Quality & Speed):": "エンコード設定 (品質と速度):",
        "Encoding Quality:": "エンコード品質:",
        "Encoding Speed:": "エンコード速度:",
        "Lossless RGB encoding active: Quality and speed presets are bypassed to guarantee exact 1:1 mathematical pixel fidelity.": "可逆RGBエンコード有効: 1:1の数学的ピクセル忠実度を保証するため、品質・速度設定はバイパスされます。",
        "When Rendering Completes:": "レンダリング完了時の動作:",
        "Do nothing": "何もしない",
        "Shutdown computer": "PCをシャットダウン",
        "Add to Render Queue & Start": "キューに追加して開始",
        "Render Queue (%1 items)": "レンダリングキュー (%1 件)",
        "Clear Finished": "完了済みをクリア",
        "No render tasks in queue": "キューにタスクはありません",
        "Use the 'New Render' tab to add a video rendering task to the background queue.": "「新規レンダリング」タブからバックグラウンドレンダリングタスクを追加してください。",
        "Cancel": "キャンセル",
        "Open Folder": "フォルダーを開く",
        "Play Video": "動画を再生",
        "Close Window": "ウィンドウを閉じる",
        "Ready - Rendering runs independently in the background": "準備完了 - レンダリングはバックグラウンドで独立実行されます",
        "%1 active tasks rendering in background...": "%1 件のタスクをバックグラウンドでレンダリング中...",
        "Draft Quality (Faster, larger compression artifacts)": "ドラフト品質 (高速、圧縮アーティファクト大)",
        "Standard Quality (Balanced, recommended for sharing)": "標準品質 (バランス良好、共有向け)",
        "High Quality (Crisp details, high bitrate - Recommended)": "高品質 (鮮明なディテール、高ビットレート - 推奨)",
        "Maximum Quality (Visually lossless, large file size)": "最高品質 (視覚的ロスレス、ファイルサイズ大)",
        "Maximum Compression (Slowest encode, smallest file)": "最大圧縮 (最高圧縮率、最も遅い)",
        "Slow (Thorough compression)": "低速 (高精度圧縮)",
        "Balanced (Medium speed and size)": "標準 (中間の速度とサイズ)",
        "Fast (Faster encoding - Recommended)": "高速 (迅速なエンコード - 推奨)",
        "Ultrafast (Instant encode, ideal for quick testing)": "超高速 (テスト用の即時エンコード)",
        "Finished": "完了",

        # Bezier Curve Editor Dialog
        "Curve Transition Editor": "カーブトランジションエディター",
        "Cubic Bezier Transition for %1": "%1 の3次ベジェトランジション",
        "Drag the control points P1 and P2 or choose a preset to sculpt value transitions over video duration.": "制御点P1とP2をドラッグするかプリセットを選択して、動画全体の値を調整します。",
        "Presets:": "プリセット:",
        "Ease": "イーズ",
        "Ease In": "イーズイン",
        "Ease Out": "イーズアウト",
        "Ease In-Out": "イーズイン・アウト",
        "Overshoot": "オーバーシュート",
        "Apply Curve": "カーブを適用"
    }
}


def detect_system_language():
    """Detect system locale and map to supported languages ('en', 'pt', 'es', 'ja')."""
    # 1. Environment variables
    for var in ("LC_ALL", "LC_MESSAGES", "LANG"):
        val = os.environ.get(var, "")
        if val:
            val = val.lower()
            if val.startswith("pt"):
                return "pt"
            if val.startswith("es"):
                return "es"
            if val.startswith("ja"):
                return "ja"
            if val.startswith("en"):
                return "en"

    # 2. QLocale
    try:
        loc = QLocale.system().name().lower()
        if loc.startswith("pt"):
            return "pt"
        if loc.startswith("es"):
            return "es"
        if loc.startswith("ja"):
            return "ja"
        if loc.startswith("en"):
            return "en"
    except Exception:
        pass

    # 3. Python standard locale
    try:
        loc, _ = locale.getdefaultlocale()
        if loc:
            loc = loc.lower()
            if loc.startswith("pt"):
                return "pt"
            if loc.startswith("es"):
                return "es"
            if loc.startswith("ja"):
                return "ja"
            if loc.startswith("en"):
                return "en"
    except Exception:
        pass

    return "en"


class I18nManager(QObject):
    """QObject exposed to QML as `i18n`."""
    languageChanged = Signal(str)

    def __init__(self, parent=None):
        super().__init__(parent)
        self._current_lang = detect_system_language()

    @Property(str, notify=languageChanged)
    def currentLanguage(self):
        return self._current_lang

    @Slot(str)
    def setLanguage(self, lang_code):
        if lang_code in ("en", "pt", "es", "ja") and lang_code != self._current_lang:
            self._current_lang = lang_code
            self.languageChanged.emit(lang_code)

    @Slot(str, result=str)
    def t(self, key):
        """Translate a string into the active language, with English fallback."""
        if not key:
            return ""
        lang_dict = TRANSLATIONS.get(self._current_lang, {})
        if key in lang_dict:
            return lang_dict[key]
        en_dict = TRANSLATIONS.get("en", {})
        return en_dict.get(key, key)

    @Slot(result=list)
    def availableLanguages(self):
        return [
            {"code": "en", "name": "English"},
            {"code": "pt", "name": "Português"},
            {"code": "es", "name": "Español"},
            {"code": "ja", "name": "日本語"}
        ]
